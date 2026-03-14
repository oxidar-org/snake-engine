# Oxidar Multiplayer Snake — Task Backlog

All completed phases (1-9) have been implemented. See `CLAUDE.md` for project reference.

## Pending Tasks

### Phase 10 — Developer MCP Server

A remote MCP server deployed on **Cloudflare Workers** that helps developers build clients for the snake game. Lives in `mcp/` within this repo and auto-deploys on every push to `master`.

#### 10.1 — Project Setup

- [x] `mcp/` directory: TypeScript project using `@modelcontextprotocol/sdk` and Cloudflare Workers
- [x] `mcp/wrangler.jsonc` configuration
- [x] `mcp/package.json`, `mcp/tsconfig.json`

#### 10.2 — MCP Tools

All tools are **read-only / developer-assistance** — no game state mutation.

- [x] `get_protocol` — Returns the full protocol spec: encoding format (MessagePack, `rmp_serde::to_vec_named`, serde internally-tagged), all client→server and server→client message types with field names, types, and semantics.
- [x] `get_game_rules` — Returns game rules: 64×32 toroidal board, no collisions/death, max 32 players, start length 4, win length 16 (earns crown, resets), 200ms tick, food respawn, 60s reconnect window.
- [x] `encode_example` — Given a message type (`join`, `turn`, `state`, `crown`, `leaderboard`, `error`) and optional field overrides, returns a MessagePack-encoded example as hex + base64, with a field-by-field breakdown.
- [x] `decode_message` — Given hex or base64 bytes, decodes as MessagePack and returns the parsed message with type identification and field descriptions.
- [x] `test_connection` — Connects to `wss://snakes.hernan.rs`, performs a WebSocket handshake, reads one `state` message, and returns connection status + parsed first frame. Useful for verifying the server is reachable.
- [x] `get_client_example` — Given a language (`python`, `javascript`, `go`, `rust`, `csharp`, `java`), returns a minimal working client that connects, sends `join`, reads state, and sends turns.

#### 10.3 — CI/CD

- [x] GitHub Action (`.github/workflows/deploy-mcp.yml`): on push to `master`, deploy `mcp/` to Cloudflare Workers via `wrangler`
- [ ] `CLOUDFLARE_API_TOKEN` repo secret for deployment
- [ ] MCP server accessible as a remote MCP endpoint (Workers URL + optional custom domain)

#### 10.4 — Documentation

- [x] README section in `mcp/` with: purpose, how to add as MCP server in Claude/clients, tool descriptions

#### Design Notes

- **No game secrets or admin tools** — the MCP server only knows what's public in the protocol spec
- **MessagePack encoding/decoding** in the Worker uses `@msgpack/msgpack` (npm)
- **WebSocket test** tool uses the Workers runtime `WebSocket` API with a timeout
- **Client examples** are static templates embedded in the Worker, parameterized by the current server URL
- **OAuth**: not needed initially — tools are public reference/utility; can add later if rate limiting is required
