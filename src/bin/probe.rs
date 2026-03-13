use std::process;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use oxidar_snake::net::protocol::{ClientMessage, ServerMessage};

const STEP_TIMEOUT: Duration = Duration::from_secs(10);

fn encode_client(msg: &ClientMessage) -> Message {
    let bytes = rmp_serde::to_vec_named(msg).expect("ClientMessage serialization should not fail");
    Message::Binary(bytes.into())
}

fn decode_server(msg: Message) -> Result<Option<ServerMessage>> {
    match msg {
        Message::Binary(bytes) => {
            let decoded: ServerMessage =
                rmp_serde::from_slice(&bytes).context("failed to decode ServerMessage")?;
            Ok(Some(decoded))
        }
        Message::Close(_) => Ok(None),
        _ => Ok(None), // ignore text/ping/pong
    }
}

fn step_label(n: u8, description: &str) -> String {
    format!("[{n}] {description:<45}")
}

fn print_ok(label: &str, detail: &str) {
    if detail.is_empty() {
        println!("{label} OK");
    } else {
        println!("{label} OK  ({detail})");
    }
}

fn print_fail(label: &str, err: &str) {
    println!("{label} FAIL\n    Error: {err}");
}

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: probe <websocket-url>");
        eprintln!("Example: probe ws://localhost:9001");
        process::exit(1);
    });

    match run(&url).await {
        Ok(()) => {
            println!("✓ All checks passed");
            process::exit(0);
        }
        Err(e) => {
            eprintln!("probe failed: {e}");
            process::exit(1);
        }
    }
}

async fn run(url: &str) -> Result<()> {
    // Step 1: Connect
    let label1 = step_label(1, &format!("Connecting to {url}..."));
    let (mut ws, _) = timeout(STEP_TIMEOUT, connect_async(url))
        .await
        .map_err(|_| {
            print_fail(&label1, "timeout");
            anyhow::anyhow!("step 1 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label1, &e.to_string());
            anyhow::anyhow!("step 1 failed: {e}")
        })?;
    print_ok(&label1, "");

    // Step 2: Join as "probe"
    let label2 = step_label(2, "Joining as \"probe\"...");
    let join_msg = encode_client(&ClientMessage::Join {
        username: "probe".into(),
    });
    timeout(STEP_TIMEOUT, ws.send(join_msg))
        .await
        .map_err(|_| {
            print_fail(&label2, "timeout");
            anyhow::anyhow!("step 2 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label2, &e.to_string());
            anyhow::anyhow!("step 2 failed: {e}")
        })?;
    print_ok(&label2, "");

    // Step 3: Wait for own snake in state
    let label3 = step_label(3, "Waiting for own snake in state...");
    let (tick, players) = timeout(STEP_TIMEOUT, wait_for_own_snake(&mut ws))
        .await
        .map_err(|_| {
            print_fail(&label3, "timeout");
            anyhow::anyhow!("step 3 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label3, &e.to_string());
            anyhow::anyhow!("step 3 failed: {e}")
        })?;
    print_ok(&label3, &format!("tick={tick}, players={players}"));

    // Step 4: Send 4 turn commands cycling Up → Right → Down → Left
    let label4 = step_label(4, "Sending turn commands...");
    let dirs: [u8; 4] = [0, 1, 2, 3]; // Up, Right, Down, Left
    let turns_sent = timeout(STEP_TIMEOUT, send_turns_with_gaps(&mut ws, &dirs))
        .await
        .map_err(|_| {
            print_fail(&label4, "timeout");
            anyhow::anyhow!("step 4 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label4, &e.to_string());
            anyhow::anyhow!("step 4 failed: {e}")
        })?;
    print_ok(&label4, &format!("{turns_sent} turns sent"));

    // Step 5: Receive 5 consecutive state messages, verify snake present
    let label5 = step_label(5, "Receiving 5 state updates...");
    let states = timeout(STEP_TIMEOUT, collect_states(&mut ws, 5))
        .await
        .map_err(|_| {
            print_fail(&label5, "timeout");
            anyhow::anyhow!("step 5 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label5, &e.to_string());
            anyhow::anyhow!("step 5 failed: {e}")
        })?;
    println!("{label5} OK");
    for (tick, players, snake_len) in &states {
        println!("    tick={tick} players={players} snake_len={snake_len}");
    }

    // Step 6: Graceful disconnect
    let label6 = step_label(6, "Disconnecting gracefully...");
    timeout(STEP_TIMEOUT, ws.close(None))
        .await
        .map_err(|_| {
            print_fail(&label6, "timeout");
            anyhow::anyhow!("step 6 failed: timeout")
        })?
        .map_err(|e| {
            print_fail(&label6, &e.to_string());
            anyhow::anyhow!("step 6 failed: {e}")
        })?;
    // Drain until server echoes close
    while let Ok(Some(Ok(msg))) = timeout(STEP_TIMEOUT, ws.next()).await {
        if matches!(msg, Message::Close(_)) {
            break;
        }
    }
    print_ok(&label6, "");

    Ok(())
}

async fn wait_for_own_snake<S>(ws: &mut S) -> Result<(u64, usize)>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = ws.next().await {
        let msg = msg.context("WebSocket error")?;
        if let Some(ServerMessage::State { tick, snakes, .. }) = decode_server(msg)?
            && snakes.iter().any(|s| s.name == "probe")
        {
            return Ok((tick, snakes.len()));
        }
    }
    bail!("connection closed before probe snake appeared");
}

async fn send_turns_with_gaps<S>(ws: &mut S, dirs: &[u8]) -> Result<usize>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error>
        + Unpin,
{
    let mut sent = 0;
    for &dir in dirs {
        // Wait for a state message before each subsequent turn
        if sent > 0 {
            wait_for_state(ws).await?;
        }
        let turn = encode_client(&ClientMessage::Turn { dir });
        ws.send(turn).await.context("failed to send Turn")?;
        sent += 1;
    }
    Ok(sent)
}

async fn wait_for_state<S>(ws: &mut S) -> Result<()>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(msg) = ws.next().await {
        let msg = msg.context("WebSocket error")?;
        if let Some(ServerMessage::State { .. }) = decode_server(msg)? {
            return Ok(());
        }
    }
    bail!("connection closed while waiting for state");
}

async fn collect_states<S>(ws: &mut S, count: usize) -> Result<Vec<(u64, usize, usize)>>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let mut results = Vec::with_capacity(count);
    while results.len() < count {
        let msg = ws
            .next()
            .await
            .context("connection closed")?
            .context("WebSocket error")?;
        if let Some(ServerMessage::State { tick, snakes, .. }) = decode_server(msg)? {
            let probe_snake = snakes.iter().find(|s| s.name == "probe");
            if let Some(snake) = probe_snake {
                results.push((tick, snakes.len(), snake.body.len()));
            } else {
                bail!("probe snake disappeared from state at tick {tick}");
            }
        }
    }
    Ok(results)
}
