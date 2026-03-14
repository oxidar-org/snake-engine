# oxidar-snake MCP Server

A remote [Model Context Protocol](https://modelcontextprotocol.io) server deployed on **Cloudflare Workers** that helps developers build clients for the [oxidar-snake](https://snakes.oxidar.org) multiplayer snake game.

## Purpose

Participants in oxidar coding sessions implement their own snake clients. This MCP server gives AI assistants (Claude, Cursor, etc.) instant access to the protocol spec, game rules, encoding utilities, and ready-to-run client examples — so the AI can help you write a working client from scratch.

## Endpoint

```
https://oxidar-snake-mcp.workers.dev/mcp
```

## Adding as a Remote MCP Server

### Claude Desktop / Claude.app

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "oxidar-snake": {
      "type": "http",
      "url": "https://oxidar-snake-mcp.workers.dev/mcp"
    }
  }
}
```

### Cursor

Open **Settings → MCP** and add:

```json
{
  "oxidar-snake": {
    "type": "http",
    "url": "https://oxidar-snake-mcp.workers.dev/mcp"
  }
}
```

### Any MCP-compatible client

The server uses the [Streamable HTTP transport](https://spec.modelcontextprotocol.io/specification/2025-03-26/basic/transports/#streamable-http) (POST to `/mcp`). No authentication required.

## Tools

| Tool | Description |
|------|-------------|
| `get_protocol` | Returns the full protocol spec: MessagePack encoding details, all client→server and server→client message types with field names, types, and semantics. |
| `get_game_rules` | Returns game rules: 64×32 toroidal board, no collisions/death, max 32 players, start length 4, win length 16 (earns crown, resets), 200ms tick rate, food respawn, 60s reconnect window. |
| `encode_example` | Given a message type (`join`, `turn`, `state`, `crown`, `leaderboard`, `error`) and optional field overrides (JSON string), returns a MessagePack-encoded example as hex + base64 with a field-by-field breakdown. |
| `decode_message` | Given hex or base64 bytes, decodes them as MessagePack and returns the parsed message with type identification and field descriptions. |
| `test_connection` | Connects to `wss://snakes.hernan.rs`, performs a WebSocket handshake, reads one `state` message, and returns connection status + parsed first frame. |
| `get_client_example` | Given a language (`python`, `javascript`, `go`, `rust`, `csharp`, `java`), returns a minimal working client that connects, sends `join`, reads state, and sends turns. |

## Development

```bash
cd mcp
npm install
npm run dev          # local dev server (wrangler dev)
npm run type-check   # TypeScript check
npm run deploy       # deploy to Cloudflare Workers (requires CLOUDFLARE_API_TOKEN)
```

## Deployment

Automatically deployed on every push to `master` that touches `mcp/**` via the GitHub Action in `.github/workflows/deploy-mcp.yml`.

Requires a `CLOUDFLARE_API_TOKEN` repository secret with Workers deployment permissions.
