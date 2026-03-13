# oxidar-snake

Multiplayer snake game server for [oxidar.org](https://oxidar.org) coding sessions. Players connect via WebSocket, control a snake on a shared board, and compete for crowns. Participants build their own clients; this repo is the authoritative server.

## Tech Stack

- Rust 2024 edition, `tokio` async runtime
- `tokio-tungstenite` (WebSocket), `rmp-serde` (MessagePack), `serde`
- `reqwest` (geo lookup), `tracing` (observability), `anyhow` (errors), `rand`

## Project Layout

```
src/
  main.rs              # entry point: config load, tracing init, server start
  config.rs            # Config/GameConfig/ServerConfig (TOML)
  leaderboard.rs       # Leaderboard::compute()
  game/
    board.rs           # Board, Position, toroidal wrapping, food
    snake.rs           # Snake, Direction, movement, growth
    engine.rs          # GameEngine: tick loop, player management, crowns
  net/
    server.rs          # WebSocket server, game loop, connection handling
    session.rs         # SessionManager, SessionId, Session enum
    protocol.rs        # ClientMessage, ServerMessage, SnakeData, encode/decode
  bin/
    probe.rs           # Deployment smoke-test client
tests/
  integration.rs       # End-to-end tests
```

## Game Rules

- 64x32 toroidal board, no collisions, no death
- Max 32 players, start length 4, win length 16 (earns a crown, resets snake)
- 200ms tick rate, food respawns on eat
- Reconnect within 60s preserves snake state (position, crowns, color, country)

## Protocol (MessagePack over WebSocket binary frames)

Encoding: `rmp_serde::to_vec_named` / `rmp_serde::from_slice`. Serde internally tagged by `"type"`.

### Client -> Server

| Message | Fields | Notes |
|---------|--------|-------|
| `join`  | `username: String` | Promotes spectator to player |
| `turn`  | `dir: u8` | 0=Up, 1=Right, 2=Down, 3=Left |

### Server -> Client

| Message | Fields | Frequency |
|---------|--------|-----------|
| `state` | `tick: u64, food: [u16;2], snakes: Vec<SnakeData>` | Every tick |
| `crown` | `name: String, crowns: u32` | On crown earned |
| `leaderboard` | `players: Vec<LeaderboardEntry>` | Every 25 ticks |
| `error` | `msg: String` | Unicast on invalid action |

`SnakeData`: `name, body: Vec<[u16;2]>, dir: u8, crowns: u32, color: String, country: Option<String>`
`LeaderboardEntry`: `name, crowns, length: u16, alive: bool, country: Option<String>`

## Deployment

- Dockerfile: multi-stage build (rust:slim -> debian:bookworm-slim + cloudflared)
- Railway: reads `PORT` env var for WebSocket port, health check on port 9002 (`/health`)
- Cloudflare tunnel: `cloudflared` runs in-container via `entrypoint.sh`, token from `CLOUDFLARE_TUNNEL_TOKEN` env var
- Hosts: `snakes.hernan.rs`, `snakes.oxidar.org`

## Development

```bash
cargo test                        # all tests
cargo test --lib                  # unit only
cargo test --test integration     # integration only
cargo clippy -- -D warnings       # lint
cargo run --bin probe -- ws://localhost:8080   # smoke test
cargo run --bin probe -- wss://snakes.hernan.rs  # smoke test production
```

## Conventions

- Commit format: `type: description` (feat/fix/test/docs/chore/refactor)
- All tests must pass before committing
- `tracing`: info for business events, warn for recoverable errors, error for unexpected failures
- Channel architecture: connections send commands via `mpsc`, game loop broadcasts via `broadcast`
