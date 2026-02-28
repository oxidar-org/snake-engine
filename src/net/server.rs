use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio::time::Interval;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::game::engine::GameEngine;
use crate::game::snake::Direction;
use crate::leaderboard;

use super::protocol::{self, ClientMessage, ServerMessage};
use super::session::{Session, SessionId, SessionManager};

/// Commands sent from connection tasks to the game loop.
#[derive(Debug)]
pub enum Command {
    Join {
        session: SessionId,
        username: String,
        reply: mpsc::Sender<ServerMessage>,
    },
    Turn {
        session: SessionId,
        dir: u8,
    },
    Disconnect {
        session: SessionId,
    },
}

/// Operation requests for session manager (run on game loop task).
#[derive(Debug)]
pub enum SessionMgrOp {
    Connect {
        reply: tokio::sync::oneshot::Sender<SessionId>,
    },
}

/// Run the full server: listener + game loop.
pub async fn run(config: Config) -> anyhow::Result<()> {
    let (cmd_tx, cmd_rx) = mpsc::channel(256);
    let (broadcast_tx, _) = broadcast::channel::<Vec<u8>>(64);
    let (session_mgr_tx, session_mgr_rx) = mpsc::channel(64);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    let health_addr: SocketAddr =
        format!("{}:{}", config.server.host, config.server.health_port).parse()?;

    let shutdown = shutdown_signal();

    let listen_cmd_tx = cmd_tx.clone();
    let listen_broadcast_tx = broadcast_tx.clone();
    let listen_session_tx = session_mgr_tx.clone();

    // Spawn the WebSocket listener
    tokio::spawn(async move {
        if let Err(e) = listen(addr, listen_cmd_tx, listen_broadcast_tx, listen_session_tx).await {
            tracing::error!(error = %e, "listener failed");
        }
    });

    // Spawn the health check listener
    tokio::spawn(async move {
        if let Err(e) = listen_health(health_addr).await {
            tracing::error!(error = %e, "health listener failed");
        }
    });

    // Run the game loop until shutdown signal
    tokio::select! {
        _ = game_loop(config, cmd_rx, broadcast_tx, session_mgr_rx) => {}
        _ = shutdown => {
            info!("shutdown signal received, exiting");
        }
    }

    Ok(())
}

/// Wait for SIGTERM or Ctrl+C.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}

/// The main game loop: processes commands, ticks, and broadcasts.
async fn game_loop(
    config: Config,
    mut cmd_rx: mpsc::Receiver<Command>,
    broadcast_tx: broadcast::Sender<Vec<u8>>,
    mut session_mgr_rx: mpsc::Receiver<SessionMgrOp>,
) {
    let rng: Box<dyn rand::Rng + Send> = {
        use rand::SeedableRng;
        Box::new(rand::rngs::StdRng::from_rng(&mut rand::rng()))
    };
    let mut engine = GameEngine::new(
        config.game.board_width,
        config.game.board_height,
        config.game.snake_start_length,
        config.game.snake_win_length,
        config.game.max_players,
        rng,
        config.game.palette.clone(),
    );
    let mut session_mgr = SessionManager::new();

    let mut tick_interval: Interval =
        tokio::time::interval(std::time::Duration::from_millis(config.game.tick_ms));
    let mut tick_count: u64 = 0;

    info!("game loop started");

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                // Tick the engine
                let result = engine.tick();
                tick_count += 1;

                // Broadcast crown events
                for crown in &result.crowns {
                    let msg: ServerMessage = crown.into();
                    let bytes = protocol::encode(&msg);
                    let _ = broadcast_tx.send(bytes);
                }

                // Broadcast state
                let state_msg: ServerMessage = (&result).into();
                let state_bytes = protocol::encode(&state_msg);
                let _ = broadcast_tx.send(state_bytes);

                // Broadcast leaderboard at interval
                if tick_count.is_multiple_of(config.game.leaderboard_interval_ticks) {
                    let entries = leaderboard::compute(&engine);
                    let lb_msg = ServerMessage::Leaderboard { players: entries };
                    let lb_bytes = protocol::encode(&lb_msg);
                    let _ = broadcast_tx.send(lb_bytes);
                }

                // Periodic debug logging
                if tick_count.is_multiple_of(100) {
                    debug!(tick = tick_count, players = engine.active_players().len(), "tick stats");
                }

                // Purge stale disconnected players
                if tick_count.is_multiple_of(50) {
                    engine.purge_stale(config.game.disconnect_timeout_s);
                }
            }

            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    Command::Join { session, username, reply } => {
                        match session_mgr.promote(session, username.clone()) {
                            Ok(()) => {
                                if let Err(e) = engine.add_player(username) {
                                    let _ = reply.send(ServerMessage::Error { msg: e.to_string() }).await;
                                }
                            }
                            Err(e) => {
                                let _ = reply.send(ServerMessage::Error { msg: e.to_string() }).await;
                            }
                        }
                    }
                    Command::Turn { session, dir } => {
                        if let Some(username) = session_username(&session_mgr, session) {
                            if let Some(direction) = Direction::from_u8(dir) {
                                engine.queue_turn(&username, direction);
                            } else {
                                warn!(dir, "invalid direction value");
                            }
                        }
                    }
                    Command::Disconnect { session } => {
                        let prev = session_mgr.disconnect(session);
                        if let Some(Session::Player { username }) = prev {
                            engine.remove_player(&username);
                        }
                    }
                }
            }

            Some(op) = session_mgr_rx.recv() => {
                match op {
                    SessionMgrOp::Connect { reply } => {
                        let id = session_mgr.connect();
                        let _ = reply.send(id);
                    }
                }
            }
        }
    }
}

fn session_username(mgr: &SessionManager, id: SessionId) -> Option<String> {
    match mgr.get(id) {
        Some(Session::Player { username }) => Some(username.clone()),
        _ => None,
    }
}

/// Start listening for WebSocket connections.
async fn listen(
    addr: SocketAddr,
    cmd_tx: mpsc::Sender<Command>,
    broadcast_tx: broadcast::Sender<Vec<u8>>,
    session_mgr_tx: mpsc::Sender<SessionMgrOp>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "WebSocket server listening");

    loop {
        let (stream, peer) = listener.accept().await?;
        info!(%peer, "new connection");

        let cmd_tx = cmd_tx.clone();
        let broadcast_tx = broadcast_tx.clone();
        let session_mgr_tx = session_mgr_tx.clone();

        tokio::spawn(async move {
            if let Err(e) =
                handle_connection(stream, peer, cmd_tx, broadcast_tx, session_mgr_tx).await
            {
                warn!(%peer, error = %e, "connection error");
            }
        });
    }
}

/// Listen for HTTP health check requests on a dedicated port.
async fn listen_health(addr: SocketAddr) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "health check listener started");

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            // Drain the incoming request
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;

            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    cmd_tx: mpsc::Sender<Command>,
    broadcast_tx: broadcast::Sender<Vec<u8>>,
    session_mgr_tx: mpsc::Sender<SessionMgrOp>,
) -> anyhow::Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_sink, mut ws_stream) = ws.split();

    // Get a session ID from the game loop
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    session_mgr_tx
        .send(SessionMgrOp::Connect { reply: reply_tx })
        .await?;
    let session_id = reply_rx.await?;

    // Subscribe to broadcasts
    let mut broadcast_rx = broadcast_tx.subscribe();

    // Channel for direct error messages to this client
    let (direct_tx, mut direct_rx) = mpsc::channel::<ServerMessage>(16);

    // Spawn broadcast forwarder
    let forward_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = broadcast_rx.recv() => {
                    match result {
                        Ok(bytes) => {
                            if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "client lagged behind broadcasts");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                Some(msg) = direct_rx.recv() => {
                    let bytes = protocol::encode(&msg);
                    if ws_sink.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Process incoming messages
    let mut is_player = false;
    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!(%peer, error = %e, "WebSocket read error");
                break;
            }
        };

        match msg {
            Message::Binary(data) => match protocol::decode(&data) {
                Ok(ClientMessage::Join { username }) => {
                    if is_player {
                        let _ = direct_tx
                            .send(ServerMessage::Error {
                                msg: "already joined".into(),
                            })
                            .await;
                        continue;
                    }
                    if username.is_empty() {
                        let _ = direct_tx
                            .send(ServerMessage::Error {
                                msg: "empty username".into(),
                            })
                            .await;
                        continue;
                    }
                    let _ = cmd_tx
                        .send(Command::Join {
                            session: session_id,
                            username,
                            reply: direct_tx.clone(),
                        })
                        .await;
                    is_player = true;
                }
                Ok(ClientMessage::Turn { dir }) => {
                    if !is_player {
                        warn!(%peer, "spectator attempted turn");
                        let _ = direct_tx
                            .send(ServerMessage::Error {
                                msg: "not a player".into(),
                            })
                            .await;
                        continue;
                    }
                    let _ = cmd_tx
                        .send(Command::Turn {
                            session: session_id,
                            dir,
                        })
                        .await;
                }
                Err(e) => {
                    warn!(%peer, error = %e, "malformed message");
                    let _ = direct_tx
                        .send(ServerMessage::Error {
                            msg: "malformed message".into(),
                        })
                        .await;
                }
            },
            Message::Text(_) => {
                warn!(%peer, "received text frame, expected binary");
                let _ = direct_tx
                    .send(ServerMessage::Error {
                        msg: "expected binary frames".into(),
                    })
                    .await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup
    let _ = cmd_tx
        .send(Command::Disconnect {
            session: session_id,
        })
        .await;
    forward_handle.abort();

    info!(%peer, "connection closed");
    Ok(())
}
