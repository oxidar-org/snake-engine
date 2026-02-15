use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use super::protocol::{self, ClientMessage, ServerMessage};
use super::session::{SessionId, SessionManager};

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

/// Shared state for the server.
pub struct Server {
    pub cmd_tx: mpsc::Sender<Command>,
    pub broadcast_tx: broadcast::Sender<Vec<u8>>,
    pub session_mgr: SessionManager,
}

impl Server {
    pub fn new() -> (Server, mpsc::Receiver<Command>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let (broadcast_tx, _) = broadcast::channel(64);
        let session_mgr = SessionManager::new();

        let server = Server {
            cmd_tx,
            broadcast_tx,
            session_mgr,
        };

        (server, cmd_rx)
    }
}

/// Start listening for WebSocket connections.
pub async fn listen(
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

/// Operation requests for session manager (run on game loop task).
#[derive(Debug)]
pub enum SessionMgrOp {
    Connect {
        reply: tokio::sync::oneshot::Sender<SessionId>,
    },
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
                        tracing::debug!(%peer, "spectator attempted turn");
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
