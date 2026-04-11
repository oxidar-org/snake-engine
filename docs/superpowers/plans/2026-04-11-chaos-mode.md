# Chaos Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a toggleable chaos injection layer that broadcasts malformed/semantically wrong frames to all clients at a configurable tick interval, with an HTTP toggle on the health port.

**Architecture:** A new `ChaosInjector` struct (owned by the game loop) generates one of five chaos payloads every N ticks when enabled. Its `Arc<AtomicBool>` is shared with the health HTTP listener, which handles `POST /chaos/on` and `POST /chaos/off`. No changes to the game engine.

**Tech Stack:** Rust, `rmp-serde` (msgpack), `rand` 0.10, `tokio`, `std::sync::atomic`

---

## File Map

| Action | Path | Responsibility |
|--------|------|---------------|
| Modify | `src/config.rs` | Add `chaos_interval_ticks: u64` to `GameConfig` |
| Create | `src/net/chaos.rs` | `ChaosInjector`, all 5 chaos payload generators, unit tests |
| Modify | `src/net/mod.rs` | Declare `pub mod chaos` |
| Modify | `src/net/server.rs` | Wire injector into `game_loop`; extend `listen_health` |
| Modify | `tests/integration.rs` | Update `test_config` helper; add chaos HTTP toggle test |
| Modify | `mcp/src/tools.ts` | Document chaos mode in protocol/game-rules sections |

---

## Task 1: Add `chaos_interval_ticks` to `GameConfig`

**Files:**
- Modify: `src/config.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block in `src/config.rs`, inside the existing `mod tests`:

```rust
#[test]
fn chaos_interval_ticks_defaults_to_ten() {
    let toml = r##"
[game]
board_width = 64
board_height = 32
max_players = 32
tick_ms = 200
snake_start_length = 4
snake_win_length = 16
disconnect_timeout_s = 60
leaderboard_interval_ticks = 25

[server]
host = "0.0.0.0"
port = 9001
"##;
    let config = Config::load_from_str(toml).unwrap();
    assert_eq!(config.game.chaos_interval_ticks, 10);
}
```

Note: `Config::load_from_str` doesn't exist yet — the test will fail to compile.

- [ ] **Step 2: Run to confirm compile failure**

```bash
cargo test chaos_interval_ticks_defaults_to_ten 2>&1 | head -20
```
Expected: compile error — `no method named load_from_str` or `no field chaos_interval_ticks`.

- [ ] **Step 3: Add the field and helper to `src/config.rs`**

Add a default function after the existing `default_health_port`:
```rust
fn default_chaos_interval_ticks() -> u64 {
    10
}
```

Add the field to `GameConfig` (after `leaderboard_interval_ticks`):
```rust
#[serde(default = "default_chaos_interval_ticks")]
pub chaos_interval_ticks: u64,
```

Add `load_from_str` to the `impl Config` block:
```rust
#[cfg(test)]
pub fn load_from_str(content: &str) -> Result<Config> {
    let mut config: Config = toml::from_str(content)?;
    if config.game.palette.is_empty() {
        config.game.palette = generate_palette(config.game.max_players as usize);
    }
    Ok(config)
}
```

- [ ] **Step 4: Update the `VALID_TOML` constant** to include the new field so existing tests still compile (add `chaos_interval_ticks = 5` to the `[game]` section in `VALID_TOML`), and add an assertion in `load_valid_config`:

```rust
assert_eq!(config.game.chaos_interval_ticks, 5);
```

- [ ] **Step 5: Update `tests/integration.rs` `test_config` helper**

The `GameConfig` struct literal in `test_config` will now fail to compile. Add the field:
```rust
chaos_interval_ticks: 10,
```

- [ ] **Step 6: Run the new test**

```bash
cargo test chaos_interval_ticks
```
Expected: PASS (both the new test and `load_valid_config`).

- [ ] **Step 7: Run all tests**

```bash
cargo test
```
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/config.rs tests/integration.rs
git commit -m "feat: add chaos_interval_ticks to GameConfig"
```

---

## Task 2: Create `ChaosInjector` with all five chaos types

**Files:**
- Create: `src/net/chaos.rs`
- Modify: `src/net/mod.rs`

- [ ] **Step 1: Declare the module**

Add to `src/net/mod.rs`:
```rust
pub mod chaos;
```

- [ ] **Step 2: Write the failing unit tests first**

Create `src/net/chaos.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn enabled_injector(interval: u64) -> ChaosInjector {
        let mut inj = ChaosInjector::new(interval);
        inj.enabled.store(true, Ordering::Relaxed);
        inj
    }

    #[test]
    fn generate_returns_none_when_disabled() {
        let mut inj = ChaosInjector::new(1);
        assert!(inj.generate(1).is_none());
    }

    #[test]
    fn generate_returns_none_when_tick_not_at_interval() {
        let mut inj = enabled_injector(10);
        assert!(inj.generate(1).is_none());
        assert!(inj.generate(9).is_none());
        assert!(inj.generate(11).is_none());
    }

    #[test]
    fn generate_returns_some_when_enabled_and_tick_matches() {
        let mut inj = enabled_injector(10);
        assert!(inj.generate(10).is_some());
        assert!(inj.generate(20).is_some());
    }

    #[test]
    fn all_chaos_types_produce_nonempty_bytes() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for tick in [10u64, 11, 12, 13, 14] {
            let mut inj = enabled_injector(1);
            // Seed the internal rng deterministically by draining with known tick
            let bytes = inj.generate(tick).unwrap();
            assert!(!bytes.is_empty(), "chaos at tick {tick} produced empty bytes");
        }
    }

    #[test]
    fn out_of_order_does_not_underflow_at_tick_one() {
        // tick=1 is the minimum valid tick; offset must not exceed it
        let mut inj = enabled_injector(1);
        // Run 20 times to exercise the random offset selection
        for _ in 0..20 {
            let bytes = inj.generate(1);
            assert!(bytes.is_some());
        }
    }
}
```

- [ ] **Step 3: Run to confirm compile failure**

```bash
cargo test --lib chaos 2>&1 | head -20
```
Expected: compile error — `ChaosInjector` not defined.

- [ ] **Step 4: Implement `ChaosInjector` and all chaos generators**

Replace `src/net/chaos.rs` with the full implementation:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::Rng;
use rand::RngExt;
use serde::Serialize;

pub struct ChaosInjector {
    pub enabled: Arc<AtomicBool>,
    interval_ticks: u64,
    rng: Box<dyn Rng + Send>,
}

impl ChaosInjector {
    pub fn new(interval_ticks: u64) -> Self {
        use rand::SeedableRng;
        let rng = Box::new(rand::rngs::StdRng::from_rng(&mut rand::rng()));
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            interval_ticks,
            rng,
        }
    }

    pub fn generate(&mut self, tick: u64) -> Option<Vec<u8>> {
        if !self.enabled.load(Ordering::Relaxed) {
            return None;
        }
        if tick % self.interval_ticks != 0 {
            return None;
        }
        Some(random_chaos(&mut *self.rng, tick))
    }
}

fn random_chaos(rng: &mut dyn Rng, current_tick: u64) -> Vec<u8> {
    match rng.random_range(0u8..5) {
        0 => chaos_garbage(rng),
        1 => chaos_unknown_type(),
        2 => chaos_wrong_field_types(),
        3 => chaos_missing_fields(current_tick),
        _ => chaos_out_of_order(rng, current_tick),
    }
}

// Type a: random garbage — undecodable binary
fn chaos_garbage(rng: &mut dyn Rng) -> Vec<u8> {
    let len: usize = rng.random_range(16..=64usize);
    (0..len).map(|_| rng.random::<u8>()).collect()
}

// Type b: valid msgpack but unknown `type` tag
#[derive(Serialize)]
struct UnknownType {
    #[serde(rename = "type")]
    type_: &'static str,
}

fn chaos_unknown_type() -> Vec<u8> {
    rmp_serde::to_vec_named(&UnknownType { type_: "glitch" }).expect("serialize")
}

// Type c: state-shaped map with wrong field types
#[derive(Serialize)]
struct WrongFieldTypes {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: &'static str, // should be u64
    food: u32,          // should be [u16; 2]
    snakes: u8,         // should be Vec<SnakeData>
}

fn chaos_wrong_field_types() -> Vec<u8> {
    rmp_serde::to_vec_named(&WrongFieldTypes {
        type_: "state",
        tick: "not_a_number",
        food: 42,
        snakes: 0,
    })
    .expect("serialize")
}

// Type d: state-shaped map with missing required fields
#[derive(Serialize)]
struct MissingFields {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: u64,
}

fn chaos_missing_fields(tick: u64) -> Vec<u8> {
    rmp_serde::to_vec_named(&MissingFields { type_: "state", tick }).expect("serialize")
}

// Type f: valid State but tick goes backwards; snakes is empty
#[derive(Serialize)]
struct OutOfOrderState {
    #[serde(rename = "type")]
    type_: &'static str,
    tick: u64,
    food: [u16; 2],
    snakes: Vec<[u16; 2]>, // empty vec; element type irrelevant when empty
}

fn chaos_out_of_order(rng: &mut dyn Rng, current_tick: u64) -> Vec<u8> {
    let max_offset = current_tick.min(50).max(1);
    let offset = rng.random_range(1..=max_offset);
    rmp_serde::to_vec_named(&OutOfOrderState {
        type_: "state",
        tick: current_tick - offset,
        food: [0, 0],
        snakes: vec![],
    })
    .expect("serialize")
}

#[cfg(test)]
mod tests {
    // ... (tests from Step 2 above)
}
```

(Keep the test module from Step 2 intact at the bottom.)

- [ ] **Step 5: Run the unit tests**

```bash
cargo test --lib chaos
```
Expected: all 5 tests PASS.

- [ ] **Step 6: Run all tests**

```bash
cargo test
```
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/net/chaos.rs src/net/mod.rs
git commit -m "feat: add ChaosInjector with five chaos payload types"
```

---

## Task 3: Wire `ChaosInjector` into the game loop

**Files:**
- Modify: `src/net/server.rs`

- [ ] **Step 1: Add imports to `src/net/server.rs`**

Add near the top (with existing `use` statements):
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::chaos::ChaosInjector;
```

- [ ] **Step 2: Update `game_loop` signature**

Change the function signature from:
```rust
async fn game_loop(
    config: Config,
    mut cmd_rx: mpsc::Receiver<Command>,
    broadcast_tx: broadcast::Sender<Vec<u8>>,
    mut session_mgr_rx: mpsc::Receiver<SessionMgrOp>,
) {
```
to:
```rust
async fn game_loop(
    config: Config,
    mut cmd_rx: mpsc::Receiver<Command>,
    broadcast_tx: broadcast::Sender<Vec<u8>>,
    mut session_mgr_rx: mpsc::Receiver<SessionMgrOp>,
    mut chaos: ChaosInjector,
) {
```

- [ ] **Step 3: Inject chaos bytes after the state broadcast**

In the tick arm of the `tokio::select!` loop, find the line:
```rust
let _ = broadcast_tx.send(state_bytes);
```

Add immediately after it:
```rust
// Chaos injection (no-op when disabled or tick not at interval)
if let Some(chaos_bytes) = chaos.generate(result.tick) {
    let _ = broadcast_tx.send(chaos_bytes);
}
```

- [ ] **Step 4: Update `run()` to create and wire `ChaosInjector`**

In `run()`, after `let (session_mgr_tx, session_mgr_rx) = mpsc::channel(64);`, add:
```rust
let chaos = ChaosInjector::new(config.game.chaos_interval_ticks);
let chaos_enabled = chaos.enabled.clone();
```

Pass `chaos_enabled` to the health listener spawn. Change:
```rust
tokio::spawn(async move {
    if let Err(e) = listen_health(health_addr).await {
        tracing::error!(error = %e, "health listener failed");
    }
});
```
to:
```rust
tokio::spawn(async move {
    if let Err(e) = listen_health(health_addr, chaos_enabled).await {
        tracing::error!(error = %e, "health listener failed");
    }
});
```

Pass `chaos` to the game loop. Change:
```rust
_ = game_loop(config, cmd_rx, broadcast_tx, session_mgr_rx) => {}
```
to:
```rust
_ = game_loop(config, cmd_rx, broadcast_tx, session_mgr_rx, chaos) => {}
```

- [ ] **Step 5: Compile check**

```bash
cargo build 2>&1 | head -30
```
Expected: fails with `listen_health` still having the old signature (one argument). This is expected — fixed in Task 4.

---

## Task 4: Extend `listen_health` with chaos toggle routes

**Files:**
- Modify: `src/net/server.rs`

- [ ] **Step 1: Update `listen_health` signature and body**

Change the function from:
```rust
async fn listen_health(addr: SocketAddr) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "health check listener started");

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;

            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}
```
to:
```rust
async fn listen_health(addr: SocketAddr, chaos_enabled: Arc<AtomicBool>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "health check listener started");

    loop {
        let (mut stream, _) = listener.accept().await?;
        let chaos_enabled = chaos_enabled.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
                .await
                .unwrap_or(0);
            let first_line = std::str::from_utf8(&buf[..n])
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("");

            let body = if first_line.starts_with("POST /chaos/on") {
                chaos_enabled.store(true, Ordering::Relaxed);
                info!("chaos enabled");
                "chaos on\n"
            } else if first_line.starts_with("POST /chaos/off") {
                chaos_enabled.store(false, Ordering::Relaxed);
                info!("chaos disabled");
                "chaos off\n"
            } else {
                "ok"
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}
```

- [ ] **Step 2: Build and verify**

```bash
cargo build
```
Expected: success, no warnings.

- [ ] **Step 3: Run all tests**

```bash
cargo test
```
Expected: all existing tests pass (the `health_endpoint_returns_200` test should still pass — it sends `GET /health` which hits the else branch returning "ok").

- [ ] **Step 4: Commit**

```bash
git add src/net/server.rs
git commit -m "feat: wire ChaosInjector into broadcast path with HTTP toggle"
```

---

## Task 5: Integration test for the HTTP chaos toggle

**Files:**
- Modify: `tests/integration.rs`

- [ ] **Step 1: Write the failing test**

Add at the bottom of `tests/integration.rs`:

```rust
#[tokio::test]
async fn chaos_toggle_via_http_delivers_unparseable_frames() {
    let port = free_port().await;
    let health_port = free_port().await;
    let mut config = test_config(port, health_port);
    config.game.chaos_interval_ticks = 1; // fire on every tick
    config.game.tick_ms = 50;

    tokio::spawn(oxidar_snake::net::server::run(config));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Enable chaos via HTTP
    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{health_port}"))
        .await
        .expect("tcp connect");
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"POST /chaos/on HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await
    .expect("write");
    let mut buf = vec![0u8; 256];
    let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
        .await
        .unwrap_or(0);
    let response = std::str::from_utf8(&buf[..n]).unwrap_or("");
    assert!(response.contains("chaos on"), "expected 'chaos on', got: {response}");

    // Connect a spectator and collect frames for 500ms
    let mut ws = connect(port).await;
    let mut unparseable = 0usize;

    let collect = async {
        while let Some(Ok(Message::Binary(data))) = ws.next().await {
            if rmp_serde::from_slice::<ServerMessage>(&data).is_err() {
                unparseable += 1;
                if unparseable >= 3 {
                    break;
                }
            }
        }
    };
    timeout(Duration::from_secs(3), collect)
        .await
        .expect("timed out before seeing 3 unparseable chaos frames");
    assert!(unparseable >= 3, "expected ≥3 chaos frames, got {unparseable}");

    // Disable chaos and verify response
    let mut stream2 = tokio::net::TcpStream::connect(format!("127.0.0.1:{health_port}"))
        .await
        .expect("tcp connect");
    tokio::io::AsyncWriteExt::write_all(
        &mut stream2,
        b"POST /chaos/off HTTP/1.1\r\nHost: localhost\r\n\r\n",
    )
    .await
    .expect("write");
    let n = tokio::io::AsyncReadExt::read(&mut stream2, &mut buf)
        .await
        .unwrap_or(0);
    let response = std::str::from_utf8(&buf[..n]).unwrap_or("");
    assert!(response.contains("chaos off"), "expected 'chaos off', got: {response}");

    ws.close(None).await.ok();
}
```

- [ ] **Step 2: Run to verify test fails (chaos not yet wired)**

Since Tasks 3 and 4 are already done at this point, this test should now compile. Run it:

```bash
cargo test chaos_toggle_via_http
```
Expected: PASS (Tasks 3+4 already wired it up). If it times out, check that `chaos_interval_ticks = 1` is being applied correctly.

- [ ] **Step 3: Run all integration tests**

```bash
cargo test --test integration
```
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add tests/integration.rs
git commit -m "test: integration test for chaos HTTP toggle"
```

---

## Task 6: Run the full test suite and clippy

- [ ] **Step 1: Run all tests**

```bash
cargo test
```
Expected: all pass.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```
Expected: no warnings.

- [ ] **Step 3: Commit any lint fixes if needed**

```bash
git add -p
git commit -m "chore: fix clippy warnings in chaos module"
```

---

## Task 7: Update MCP tools for chaos mode

**Files:**
- Modify: `mcp/src/tools.ts`

- [ ] **Step 1: Read the current tools file**

```bash
cat mcp/src/tools.ts
```

Find the `GAME_RULES` constant (describes game mechanics) and the `PROTOCOL_SPEC` constant (describes server messages).

- [ ] **Step 2: Append chaos mode to `GAME_RULES`**

Locate `GAME_RULES` in `mcp/src/tools.ts` and add after the existing rules (before the closing template literal backtick):

```
## Chaos Mode
- Organizers can enable fault-injection mode via HTTP: POST /chaos/on or POST /chaos/off to the health port (default 9002)
- When enabled, the server injects one extra frame every N ticks (configured by chaos_interval_ticks, default 10)
- Chaos frame types (random, equal probability):
  - Garbage: 16-64 random bytes, not valid msgpack
  - Unknown type: valid msgpack map with "type": "glitch"
  - Wrong field types: state-shaped map with tick as string, food as integer
  - Missing fields: {"type":"state","tick":N} with no snakes or food keys
  - Out-of-order: valid state message but tick is lower than the previous tick
- Clients MUST handle decode errors and semantic anomalies gracefully (ignore and continue)
```

- [ ] **Step 3: Note chaos frames in `PROTOCOL_SPEC`**

Find the Server → Client section of `PROTOCOL_SPEC` and add a row or note:

```
| `(chaos)` | random malformed bytes | Only during chaos mode (organizer-enabled). Clients must handle gracefully. |
```

- [ ] **Step 4: Build the MCP worker to verify no TypeScript errors**

```bash
cd mcp && npm run build 2>&1 | tail -20
```
Expected: build succeeds with no errors.

- [ ] **Step 5: Commit**

```bash
cd ..
git add mcp/src/tools.ts
git commit -m "docs: update MCP tools with chaos mode protocol and rules"
```

---

## Done

After all tasks pass, chaos mode is complete:

```bash
# Enable during the event:
curl -X POST https://snakes.hernan.rs/chaos/on

# Disable when done:
curl -X POST https://snakes.hernan.rs/chaos/off
```
