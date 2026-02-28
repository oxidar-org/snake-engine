use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use oxidar_snake::config::{Config, GameConfig, ServerConfig};
use oxidar_snake::net::protocol::ServerMessage;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

fn test_config(port: u16, health_port: u16) -> Config {
    Config {
        game: GameConfig {
            board_width: 16,
            board_height: 16,
            max_players: 4,
            tick_ms: 50,
            snake_start_length: 4,
            snake_win_length: 16,
            disconnect_timeout_s: 5,
            leaderboard_interval_ticks: 5,
            palette: vec!["#FF0000".into(), "#00FF00".into(), "#0000FF".into()],
        },
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port,
            health_port,
        },
    }
}

async fn connect(
    port: u16,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://127.0.0.1:{port}");
    let (ws, _) = timeout(
        Duration::from_secs(2),
        tokio_tungstenite::connect_async(&url),
    )
    .await
    .expect("connect timeout")
    .expect("connect failed");
    ws
}

fn encode_join(username: &str) -> Vec<u8> {
    rmp_serde::to_vec_named(&std::collections::HashMap::from([
        ("type", "join"),
        ("username", username),
    ]))
    .unwrap()
}

fn encode_turn(dir: u8) -> Vec<u8> {
    #[derive(serde::Serialize)]
    struct Turn {
        r#type: String,
        dir: u8,
    }
    rmp_serde::to_vec_named(&Turn {
        r#type: "turn".into(),
        dir,
    })
    .unwrap()
}

async fn recv_server_msg(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> ServerMessage {
    loop {
        let msg = timeout(Duration::from_secs(3), ws.next())
            .await
            .expect("recv timeout")
            .expect("stream ended")
            .expect("ws error");
        if let Message::Binary(data) = msg {
            return rmp_serde::from_slice(&data).expect("decode server message");
        }
    }
}

async fn recv_state(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> ServerMessage {
    loop {
        let msg = recv_server_msg(ws).await;
        if matches!(msg, ServerMessage::State { .. }) {
            return msg;
        }
    }
}

/// Find a free port by binding to :0
async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

#[tokio::test]
async fn two_players_join_move_disconnect_reconnect() {
    let port = free_port().await;
    let health_port = free_port().await;
    let config = test_config(port, health_port);

    // Start server in background
    tokio::spawn(oxidar_snake::net::server::run(config));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect 2 clients and join
    let mut ws1 = connect(port).await;
    let mut ws2 = connect(port).await;

    ws1.send(Message::Binary(encode_join("alice").into()))
        .await
        .unwrap();
    ws2.send(Message::Binary(encode_join("bob").into()))
        .await
        .unwrap();

    // Wait for a state message — both snakes should be present
    tokio::time::sleep(Duration::from_millis(200)).await;
    let state = recv_state(&mut ws1).await;
    match &state {
        ServerMessage::State { snakes, .. } => {
            let names: Vec<&str> = snakes.iter().map(|s| s.name.as_str()).collect();
            assert!(names.contains(&"alice"), "alice missing from state");
            assert!(names.contains(&"bob"), "bob missing from state");
        }
        _ => panic!("expected State"),
    }

    // Send turn from client 1
    ws1.send(Message::Binary(encode_turn(0).into()))
        .await
        .unwrap();

    // Get another state — both should still be there and moving
    let state1 = recv_state(&mut ws1).await;
    let state2 = recv_state(&mut ws1).await;
    match (&state1, &state2) {
        (ServerMessage::State { snakes: s1, .. }, ServerMessage::State { snakes: s2, .. }) => {
            assert_eq!(s1.len(), 2);
            assert_eq!(s2.len(), 2);
            // Verify movement: heads should differ between ticks
            let alice1 = s1.iter().find(|s| s.name == "alice").unwrap();
            let alice2 = s2.iter().find(|s| s.name == "alice").unwrap();
            assert_ne!(alice1.body[0], alice2.body[0], "alice should be moving");
        }
        _ => panic!("expected State messages"),
    }

    // Disconnect client 1
    ws1.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Drain buffered messages from ws2, keep reading until we get a state with only 1 snake
    let state = timeout(Duration::from_secs(3), async {
        loop {
            let msg = recv_state(&mut ws2).await;
            if let ServerMessage::State { ref snakes, .. } = msg
                && snakes.len() == 1
            {
                return msg;
            }
        }
    })
    .await
    .expect("timed out waiting for state with 1 snake");
    match &state {
        ServerMessage::State { snakes, .. } => {
            assert_eq!(snakes[0].name, "bob");
        }
        _ => panic!("expected State"),
    }

    // Reconnect client 1
    let mut ws1_new = connect(port).await;
    ws1_new
        .send(Message::Binary(encode_join("alice").into()))
        .await
        .unwrap();
    // Wait for state with both snakes
    let state = timeout(Duration::from_secs(3), async {
        loop {
            let msg = recv_state(&mut ws2).await;
            if let ServerMessage::State { ref snakes, .. } = msg
                && snakes.len() == 2
            {
                return msg;
            }
        }
    })
    .await
    .expect("timed out waiting for reconnected state");
    match &state {
        ServerMessage::State { snakes, .. } => {
            let names: Vec<&str> = snakes.iter().map(|s| s.name.as_str()).collect();
            assert!(
                names.contains(&"alice"),
                "alice should be back after reconnect"
            );
            assert!(names.contains(&"bob"), "bob should still be present");
        }
        _ => panic!("expected State"),
    }

    ws1_new.close(None).await.ok();
    ws2.close(None).await.ok();
}

#[tokio::test]
async fn spectator_receives_state_without_joining() {
    let port = free_port().await;
    let health_port = free_port().await;
    let config = test_config(port, health_port);

    tokio::spawn(oxidar_snake::net::server::run(config));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect as spectator (no join)
    let mut ws = connect(port).await;

    // Should still receive state broadcasts
    let state = recv_state(&mut ws).await;
    assert!(matches!(state, ServerMessage::State { .. }));

    ws.close(None).await.ok();
}

#[tokio::test]
async fn health_endpoint_returns_200() {
    let port = free_port().await;
    let health_port = free_port().await;
    let config = test_config(port, health_port);

    tokio::spawn(oxidar_snake::net::server::run(config));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a plain HTTP GET /health request to the dedicated health port
    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{health_port}"))
        .await
        .expect("tcp connect");
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await
    .expect("write request");

    let mut buf = vec![0u8; 1024];
    let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
        .await
        .expect("read response");
    let response = std::str::from_utf8(&buf[..n]).expect("valid utf-8");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
    assert!(
        response.ends_with("ok"),
        "body should be 'ok', got: {response}"
    );
}
