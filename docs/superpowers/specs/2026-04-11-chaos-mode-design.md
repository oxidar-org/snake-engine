# Chaos Mode — Design Spec

**Date:** 2026-04-11
**Status:** Approved

## Summary

Add a toggleable chaos injection layer to the game server. When enabled, the server periodically broadcasts malformed or semantically wrong messages to all connected clients, forcing participants to write fault-tolerant client code.

## Goals

- All clients receive the same chaos at the same time
- Organizers can toggle chaos on/off at runtime via HTTP without restarting
- Chaos fires on a fixed tick interval, configurable in TOML
- No changes to the game engine or game logic

## Architecture

### New file: `src/net/chaos.rs`

`ChaosInjector` struct:
- `enabled: Arc<AtomicBool>` — shared with the HTTP toggle handler
- `interval_ticks: u64` — how often chaos fires

Single method: `generate(tick: u64, rng: &mut dyn Rng) -> Option<Vec<u8>>`
- Returns `None` if disabled or `tick % interval_ticks != 0`
- Otherwise picks one of 5 chaos types at random and returns serialized bytes

All chaos serialization uses `rmp_serde` with private `#[derive(Serialize)]` structs — no new dependencies.

### Changes to `src/net/server.rs`

- `game_loop` calls `injector.generate(tick, rng)` after the normal state broadcast; if `Some(bytes)` is returned, those bytes are broadcast immediately after
- `listen_health` accepts `Arc<AtomicBool>` and handles two new routes alongside the existing health check

### Changes to `src/config.rs`

`GameConfig` gets one new field:
```toml
chaos_interval_ticks = 10   # default: 10 ticks between chaos injections
```

Chaos starts **off** at boot regardless of config. The config only controls the interval.

### Changes to `src/main.rs`

Wires `Arc<AtomicBool>` from `ChaosInjector::new()` to both `game_loop` and `listen_health`.

## Chaos Types

Five types, selected with equal probability each time chaos fires:

| ID | Name | Payload | Client must handle |
|----|------|---------|-------------------|
| a | Garbage bytes | 16–64 random bytes | `from_slice` decode failure |
| b | Unknown type | Valid msgpack, `"type": "glitch"` | Unknown variant in type tag |
| c | Wrong field types | State-shaped map, `tick` is a string, `food` is a single integer | Type mismatch on deserialization |
| d | Missing fields | `{"type": "state", "tick": 99}` — no `snakes`, no `food` | Missing required field error |
| f | Out-of-order tick | Valid `State`, all fields correct, `tick` = current − rand(1..min(50, current)), empty `snakes` | Tick counter going backwards |

## HTTP Toggle

Routes added to the health port listener (default 9002):

```
POST /chaos/on    → sets AtomicBool to true,  responds "chaos on\n"
POST /chaos/off   → sets AtomicBool to false, responds "chaos off\n"
GET  /health      → existing 200 OK (any other path)
```

Minimal HTTP parse — read first line, match method and path. No framework.

Organizer usage:
```bash
curl -X POST https://snakes.hernan.rs/chaos/on
curl -X POST https://snakes.hernan.rs/chaos/off
```

## Config Example

```toml
[game]
# ... existing fields ...
chaos_interval_ticks = 10
```

## Testing

- Unit tests in `chaos.rs`: verify `generate` returns `None` when disabled, returns bytes at correct interval, covers all 5 types
- Integration test: connect a client, enable chaos via HTTP, verify malformed frames arrive at the expected interval
