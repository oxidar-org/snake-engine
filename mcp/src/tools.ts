import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { encode as msgpackEncode, decode as msgpackDecode } from "@msgpack/msgpack";

// ─── Byte utilities (Web-standard; no Node Buffer needed) ────────────────────

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2)
    out[i / 2] = parseInt(hex.slice(i, i + 2), 16);
  return out;
}

function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

const SERVER_URL = "wss://snakes.hernan.rs";

// ─── Static content ──────────────────────────────────────────────────────────

const PROTOCOL_SPEC = `# Oxidar Snake — Protocol Specification

## Transport
- WebSocket binary frames
- Encoding: MessagePack via \`rmp_serde::to_vec_named\` (Rust server)
- Decoding: \`rmp_serde::from_slice\`
- Serde tagging: internally tagged by field "type"
- Use named fields (map format), NOT array/positional format

## Client → Server Messages

### join
Promotes a spectator connection to an active player.
Fields:
  - type: "join"
  - username: string  (display name shown to other players)

### turn
Changes the snake's direction. Ignored if opposite of current direction.
Fields:
  - type: "turn"
  - dir: u8  (0=Up, 1=Right, 2=Down, 3=Left)

## Server → Client Messages

### state  (every tick, ~200ms)
Full game snapshot.
Fields:
  - type: "state"
  - tick: u64  (monotonically increasing counter)
  - food: [u16, u16]  (x, y position of current food)
  - snakes: SnakeData[]

SnakeData fields:
  - name: string
  - body: [[u16, u16], ...]  (head first; each element is [x, y])
  - dir: u8  (current direction: 0=Up 1=Right 2=Down 3=Left)
  - crowns: u32  (crowns earned by this player)
  - color: string  (CSS hex color, e.g. "#ff5500")
  - country: string | null  (ISO country code from geo-lookup, e.g. "AR")

### crown  (unicast, on crown earned)
Sent to all players when someone wins a crown.
Fields:
  - type: "crown"
  - name: string  (player who earned the crown)
  - crowns: u32   (their new total)

### leaderboard  (every 25 ticks)
Fields:
  - type: "leaderboard"
  - players: LeaderboardEntry[]

LeaderboardEntry fields:
  - name: string
  - crowns: u32
  - length: u16  (current snake length)
  - alive: bool  (false if disconnected but within reconnect window)
  - country: string | null

### error  (unicast, on invalid action)
Fields:
  - type: "error"
  - msg: string

## Encoding Notes
- All integers use MessagePack's native integer encoding (no string conversion)
- Arrays like \`body\` and \`food\` use MessagePack arrays
- Null fields are encoded as MessagePack nil (not omitted)
- The "type" field must be the first field in the map for compatibility
`;

const GAME_RULES = `# Oxidar Snake — Game Rules

## Board
- Size: 64 × 32 cells (width × height)
- Topology: toroidal (wrapping) — snakes that exit one edge appear on the opposite edge
- Coordinate origin: top-left (0,0); x increases right, y increases down

## Players
- Maximum players: 32
- New connections start as spectators (they receive state but cannot move)
- Send a "join" message to become a player

## Snake Mechanics
- Starting length: 4 cells
- Movement: one cell per tick in the current direction
- No collisions and no death — snakes pass through each other and themselves
- Direction changes: one per tick; cannot reverse (e.g. cannot go Left when moving Right)

## Winning (Crowns)
- Win condition: reach length 16
- Reward: earn one crown, snake immediately resets to starting length (4)
- The game continues indefinitely; players compete for total crown count

## Food
- One food item on the board at all times
- Eating food: snake grows by one cell
- On eat: food respawns at a random empty cell

## Tick Rate
- 200ms per tick (5 ticks/second)
- All snakes move simultaneously on each tick

## Reconnection
- If a client disconnects and reconnects within 60 seconds, their snake state is fully preserved:
  position, length, direction, crowns, color, country code

## Spectating
- Connections that have not sent "join" receive all server→client messages (state, crown, leaderboard)
  but cannot send turn commands
`;

// ─── Default message examples ─────────────────────────────────────────────────

type MessageRecord = Record<string, unknown>;

const DEFAULT_MESSAGES: Record<string, MessageRecord> = {
  join: { type: "join", username: "player1" },
  turn: { type: "turn", dir: 1 },
  state: {
    type: "state",
    tick: 42,
    food: [31, 15],
    snakes: [
      {
        name: "player1",
        body: [[10, 8], [9, 8], [8, 8], [7, 8]],
        dir: 1,
        crowns: 2,
        color: "#ff5500",
        country: "AR",
      },
    ],
  },
  crown: { type: "crown", name: "player1", crowns: 3 },
  leaderboard: {
    type: "leaderboard",
    players: [
      { name: "player1", crowns: 3, length: 7, alive: true, country: "AR" },
      { name: "player2", crowns: 1, length: 4, alive: true, country: "US" },
    ],
  },
  error: { type: "error", msg: "unknown command" },
};

// ─── Field descriptions ───────────────────────────────────────────────────────

const FIELD_DESCRIPTIONS: Record<string, Record<string, string>> = {
  join: {
    type: 'always "join"',
    username: "display name for this player",
  },
  turn: {
    type: 'always "turn"',
    dir: "direction: 0=Up 1=Right 2=Down 3=Left",
  },
  state: {
    type: 'always "state"',
    tick: "monotonically increasing tick counter (u64)",
    food: "[x, y] position of the current food item",
    snakes: "array of SnakeData objects (see protocol spec)",
  },
  crown: {
    type: 'always "crown"',
    name: "player who earned the crown",
    crowns: "their new total crown count",
  },
  leaderboard: {
    type: 'always "leaderboard"',
    players: "array of LeaderboardEntry objects",
  },
  error: {
    type: 'always "error"',
    msg: "human-readable error description",
  },
};

// ─── Client examples ──────────────────────────────────────────────────────────

const CLIENT_EXAMPLES: Record<string, string> = {
  python: `#!/usr/bin/env python3
"""Minimal oxidar-snake client — Python"""
import asyncio
import msgpack          # pip install msgpack
import websockets       # pip install websockets

SERVER = "wss://snakes.hernan.rs"

def encode(msg: dict) -> bytes:
    return msgpack.packb(msg, use_bin_type=True)

def decode(data: bytes) -> dict:
    return msgpack.unpackb(data, raw=False)

async def main():
    async with websockets.connect(SERVER) as ws:
        # Become a player
        await ws.send(encode({"type": "join", "username": "my_snake"}))

        direction = 1  # 0=Up  1=Right  2=Down  3=Left

        async for raw in ws:
            msg = decode(raw)
            if msg["type"] != "state":
                continue

            snakes = msg["snakes"]
            food   = msg["food"]

            # TODO: implement your AI here
            # Example: always turn toward food
            me = next((s for s in snakes if s["name"] == "my_snake"), None)
            if me:
                head = me["body"][0]
                dx = food[0] - head[0]
                dy = food[1] - head[1]
                if abs(dx) >= abs(dy):
                    direction = 1 if dx > 0 else 3  # Right / Left
                else:
                    direction = 2 if dy > 0 else 0  # Down  / Up

            await ws.send(encode({"type": "turn", "dir": direction}))

asyncio.run(main())
`,

  javascript: `// Minimal oxidar-snake client — Node.js
// npm install @msgpack/msgpack ws
import { encode, decode } from "@msgpack/msgpack";
import WebSocket from "ws";

const SERVER = "wss://snakes.hernan.rs";
const ws = new WebSocket(SERVER);

ws.on("open", () => {
  ws.send(encode({ type: "join", username: "my_snake" }));
});

let direction = 1; // 0=Up  1=Right  2=Down  3=Left

ws.on("message", (data) => {
  const msg = decode(data);
  if (msg.type !== "state") return;

  const { snakes, food } = msg;
  const me = snakes.find((s) => s.name === "my_snake");

  if (me) {
    const [hx, hy] = me.body[0];
    const [fx, fy] = food;
    const dx = fx - hx;
    const dy = fy - hy;

    // TODO: implement your AI here
    if (Math.abs(dx) >= Math.abs(dy)) {
      direction = dx > 0 ? 1 : 3; // Right / Left
    } else {
      direction = dy > 0 ? 2 : 0; // Down  / Up
    }
  }

  ws.send(encode({ type: "turn", dir: direction }));
});

ws.on("error", console.error);
`,

  go: `// Minimal oxidar-snake client — Go
// go get github.com/gorilla/websocket github.com/vmihailenco/msgpack/v5
package main

import (
\t"log"

\t"github.com/gorilla/websocket"
\t"github.com/vmihailenco/msgpack/v5"
)

const server = "wss://snakes.hernan.rs"

func encode(v any) []byte {
\tb, _ := msgpack.Marshal(v)
\treturn b
}

func main() {
\tconn, _, err := websocket.DefaultDialer.Dial(server, nil)
\tif err != nil {
\t\tlog.Fatal(err)
\t}
\tdefer conn.Close()

\tconn.WriteMessage(websocket.BinaryMessage, encode(map[string]any{
\t\t"type":     "join",
\t\t"username": "my_snake",
\t}))

\tdirection := 1 // 0=Up  1=Right  2=Down  3=Left

\tfor {
\t\t_, data, err := conn.ReadMessage()
\t\tif err != nil {
\t\t\tlog.Fatal(err)
\t\t}

\t\tvar msg map[string]any
\t\tif err := msgpack.Unmarshal(data, &msg); err != nil || msg["type"] != "state" {
\t\t\tcontinue
\t\t}

\t\t// TODO: implement your AI here — use msg["snakes"] and msg["food"]

\t\tconn.WriteMessage(websocket.BinaryMessage, encode(map[string]any{
\t\t\t"type": "turn",
\t\t\t"dir":  direction,
\t\t}))
\t}
}
`,

  rust: `// Minimal oxidar-snake client — Rust
// [dependencies]
// tokio = { version = "1", features = ["full"] }
// tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
// rmp-serde = "1"
// serde = { version = "1", features = ["derive"] }
// serde_json = "1"
use std::collections::HashMap;

use rmp_serde as rmps;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use futures_util::{SinkExt, StreamExt};

const SERVER: &str = "wss://snakes.hernan.rs";

#[tokio::main]
async fn main() {
    let (mut ws, _) = connect_async(SERVER).await.expect("connect");

    // Join as a player
    let join = rmps::to_vec_named(&serde_json::json!({"type": "join", "username": "my_snake"}))
        .expect("encode");
    ws.send(Message::Binary(join.into())).await.expect("send join");

    let mut direction: u8 = 1; // 0=Up  1=Right  2=Down  3=Left

    while let Some(Ok(Message::Binary(data))) = ws.next().await {
        let msg: serde_json::Value = rmps::from_slice(&data).expect("decode");
        if msg["type"] != "state" { continue; }

        // TODO: implement your AI here
        // Access msg["snakes"] and msg["food"] for game state

        let turn = rmps::to_vec_named(&serde_json::json!({"type": "turn", "dir": direction}))
            .expect("encode");
        ws.send(Message::Binary(turn.into())).await.expect("send turn");
    }
}
`,

  csharp: `// Minimal oxidar-snake client — C#
// dotnet add package MessagePack
// dotnet add package Websocket.Client
using MessagePack;
using Websocket.Client;

var server = new Uri("wss://snakes.hernan.rs");
var direction = 1; // 0=Up  1=Right  2=Down  3=Left

using var client = new WebsocketClient(server);
client.MessageReceived.Subscribe(msg =>
{
    if (msg.Binary is null) return;

    var state = MessagePackSerializer.Deserialize<Dictionary<string, object>>(msg.Binary);
    if (state["type"].ToString() != "state") return;

    // TODO: implement your AI here — read state["snakes"] and state["food"]

    var turn = MessagePackSerializer.Serialize(new Dictionary<string, object>
    {
        ["type"] = "turn",
        ["dir"]  = direction,
    });
    client.Send(turn);
});

await client.Start();

// Send join
var join = MessagePackSerializer.Serialize(new Dictionary<string, object>
{
    ["type"]     = "join",
    ["username"] = "my_snake",
});
client.Send(join);

await Task.Delay(Timeout.Infinite);
`,

  java: `// Minimal oxidar-snake client — Java
// Maven: org.java-websocket:Java-WebSocket:1.5.4
//        org.msgpack:msgpack-core:0.9.8
import org.java_websocket.client.WebSocketClient;
import org.java_websocket.handshake.ServerHandshake;
import org.msgpack.core.MessagePack;
import org.msgpack.core.MessageUnpacker;
import org.msgpack.core.MessageBufferPacker;

import java.net.URI;
import java.nio.ByteBuffer;
import java.util.Map;

public class SnakeClient extends WebSocketClient {
    private int direction = 1; // 0=Up  1=Right  2=Down  3=Left

    public SnakeClient(URI uri) { super(uri); }

    @Override
    public void onOpen(ServerHandshake hs) {
        send(encode(Map.of("type", "join", "username", "my_snake")));
    }

    @Override
    public void onMessage(ByteBuffer bytes) {
        try (MessageUnpacker u = MessagePack.newDefaultUnpacker(bytes)) {
            // Decode and check type field
            int size = u.unpackMapHeader();
            String type = null;
            for (int i = 0; i < size; i++) {
                String key = u.unpackString();
                if (key.equals("type")) type = u.unpackString();
                else u.skipValue();
            }
            if (!"state".equals(type)) return;

            // TODO: implement your AI here
            send(encode(Map.of("type", "turn", "dir", direction)));
        } catch (Exception e) { e.printStackTrace(); }
    }

    @Override public void onClose(int c, String r, boolean remote) {}
    @Override public void onError(Exception e) { e.printStackTrace(); }

    private byte[] encode(Map<String, Object> msg) {
        try (MessageBufferPacker p = MessagePack.newDefaultBufferPacker()) {
            p.packMapHeader(msg.size());
            for (var entry : msg.entrySet()) {
                p.packString(entry.getKey());
                Object v = entry.getValue();
                if (v instanceof String s) p.packString(s);
                else if (v instanceof Integer i) p.packInt(i);
            }
            return p.toByteArray();
        } catch (Exception e) { throw new RuntimeException(e); }
    }

    public static void main(String[] args) throws Exception {
        var client = new SnakeClient(new URI("wss://snakes.hernan.rs"));
        client.connectBlocking();
        Thread.currentThread().join();
    }
}
`,
};

// ─── Tool registration ────────────────────────────────────────────────────────

export function registerTools(server: McpServer): void {
  // ── get_protocol ─────────────────────────────────────────────────────────
  server.tool(
    "get_protocol",
    "Returns the full protocol specification for the oxidar-snake game server: " +
      "encoding format (MessagePack, rmp_serde::to_vec_named, serde internally-tagged), " +
      "all client→server and server→client message types with field names, types, and semantics.",
    async () => ({
      content: [{ type: "text" as const, text: PROTOCOL_SPEC }],
    }),
  );

  // ── get_game_rules ────────────────────────────────────────────────────────
  server.tool(
    "get_game_rules",
    "Returns the game rules for oxidar-snake: board dimensions (64×32 toroidal), " +
      "no collisions/death, max 32 players, start length 4, win length 16 (earns crown, resets), " +
      "200ms tick rate, food respawn, and 60-second reconnect window.",
    async () => ({
      content: [{ type: "text" as const, text: GAME_RULES }],
    }),
  );

  // ── encode_example ────────────────────────────────────────────────────────
  server.tool(
    "encode_example",
    "Given a message type and optional field overrides, encodes a MessagePack example " +
      "and returns it as hex + base64 with a field-by-field breakdown. " +
      "Supported types: join, turn, state, crown, leaderboard, error.",
    {
      message_type: z
        .enum(["join", "turn", "state", "crown", "leaderboard", "error"])
        .describe("The message type to encode"),
      overrides: z
        .string()
        .optional()
        .describe("JSON object string of field overrides (e.g. '{\"username\":\"alice\"}')"),
    },
    async ({ message_type, overrides }) => {
      let overrideObj: MessageRecord = {};
      if (overrides) {
        try {
          overrideObj = JSON.parse(overrides) as MessageRecord;
        } catch {
          return {
            content: [{ type: "text" as const, text: "Error: overrides must be valid JSON" }],
            isError: true,
          };
        }
      }

      const base = DEFAULT_MESSAGES[message_type] ?? { type: message_type };
      const msg: MessageRecord = { ...base, ...overrideObj };

      const bytes = msgpackEncode(msg);
      const hex = bytesToHex(bytes);
      const b64 = bytesToBase64(bytes);

      const fieldDesc = FIELD_DESCRIPTIONS[message_type] ?? {};
      const breakdown = Object.entries(msg)
        .map(([k, v]) => {
          const desc = fieldDesc[k] ? `  // ${fieldDesc[k]}` : "";
          return `  ${k}: ${JSON.stringify(v)}${desc}`;
        })
        .join("\n");

      const text =
        `Message type: ${message_type}\n` +
        `Bytes: ${bytes.length}\n\n` +
        `Hex:    ${hex}\n` +
        `Base64: ${b64}\n\n` +
        `Field breakdown:\n${breakdown}`;

      return { content: [{ type: "text" as const, text }] };
    },
  );

  // ── decode_message ────────────────────────────────────────────────────────
  server.tool(
    "decode_message",
    "Given hex or base64 bytes, decodes them as MessagePack and returns the parsed message " +
      "with type identification and field descriptions.",
    {
      data: z
        .string()
        .describe(
          "MessagePack-encoded bytes as a hex string (e.g. '81a474797065...') " +
            "or base64 string. Auto-detected by character set.",
        ),
    },
    async ({ data }) => {
      const trimmed = data.trim().replace(/\s+/g, "");
      let bytes: Uint8Array;

      if (/^[0-9a-fA-F]+$/.test(trimmed)) {
        bytes = hexToBytes(trimmed);
      } else {
        bytes = base64ToBytes(trimmed);
      }

      let decoded: unknown;
      try {
        decoded = msgpackDecode(bytes);
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Failed to decode MessagePack: ${err instanceof Error ? err.message : String(err)}`,
            },
          ],
          isError: true,
        };
      }

      const msgType =
        decoded !== null &&
        typeof decoded === "object" &&
        "type" in (decoded as object)
          ? String((decoded as MessageRecord)["type"])
          : "unknown";

      const fieldDesc = FIELD_DESCRIPTIONS[msgType] ?? {};
      let breakdown = "";
      if (decoded !== null && typeof decoded === "object" && !Array.isArray(decoded)) {
        breakdown =
          "\n\nField breakdown:\n" +
          Object.entries(decoded as MessageRecord)
            .map(([k, v]) => {
              const desc = fieldDesc[k] ? `  // ${fieldDesc[k]}` : "";
              return `  ${k}: ${JSON.stringify(v)}${desc}`;
            })
            .join("\n");
      }

      const json = JSON.stringify(decoded, null, 2);
      const text = `Decoded MessagePack (${bytes.length} bytes)\nType: ${msgType}\n\n${json}${breakdown}`;

      return { content: [{ type: "text" as const, text }] };
    },
  );

  // ── test_connection ───────────────────────────────────────────────────────
  server.tool(
    "test_connection",
    `Connects to ${SERVER_URL}, performs a WebSocket handshake, reads one state message, ` +
      "and returns connection status + parsed first frame. Useful for verifying the server is reachable.",
    async () => {
      const TIMEOUT_MS = 8000;

      let wsResponse: Response;
      try {
        wsResponse = await fetch(SERVER_URL, {
          headers: { Upgrade: "websocket" },
        });
      } catch (err) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Connection failed: ${err instanceof Error ? err.message : String(err)}`,
            },
          ],
          isError: true,
        };
      }

      // Cloudflare Workers exposes the WebSocket on the response object
      const ws = (wsResponse as unknown as { webSocket: WebSocket | null }).webSocket;
      if (!ws) {
        return {
          content: [
            {
              type: "text" as const,
              text: `Server responded with HTTP ${wsResponse.status} but did not upgrade to WebSocket`,
            },
          ],
          isError: true,
        };
      }

      ws.accept();

      type WsResult = { ok: true; parsed: unknown } | { ok: false; reason: string };

      const result = await new Promise<WsResult>((resolve) => {
        const timer = setTimeout(() => {
          ws.close();
          resolve({ ok: false, reason: "Timed out waiting for state message" });
        }, TIMEOUT_MS);

        ws.addEventListener("message", (event) => {
          clearTimeout(timer);
          ws.close();
          try {
            const raw =
              typeof event.data === "string"
                ? new TextEncoder().encode(event.data)
                : new Uint8Array(event.data as ArrayBuffer);
            const parsed = msgpackDecode(raw);
            resolve({ ok: true, parsed });
          } catch {
            resolve({ ok: true, parsed: "(binary frame received, but decode failed)" });
          }
        });

        ws.addEventListener("error", () => {
          clearTimeout(timer);
          resolve({ ok: false, reason: "WebSocket error during connection" });
        });
      });

      if (!result.ok) {
        return {
          content: [{ type: "text" as const, text: `Connection error: ${result.reason}` }],
          isError: true,
        };
      }

      const json = JSON.stringify(result.parsed, null, 2);
      const text = `Connected to ${SERVER_URL} successfully.\n\nFirst frame:\n${json}`;
      return { content: [{ type: "text" as const, text }] };
    },
  );

  // ── get_client_example ────────────────────────────────────────────────────
  server.tool(
    "get_client_example",
    "Given a programming language, returns a minimal working snake client that connects to " +
      `${SERVER_URL}, sends a join message, reads game state, and sends turn commands. ` +
      "Supported: python, javascript, go, rust, csharp, java.",
    {
      language: z
        .enum(["python", "javascript", "go", "rust", "csharp", "java"])
        .describe("Target programming language"),
    },
    async ({ language }) => {
      const example = CLIENT_EXAMPLES[language];
      return { content: [{ type: "text" as const, text: example }] };
    },
  );
}
