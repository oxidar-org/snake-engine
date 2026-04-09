import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import { registerTools } from "./tools.js";

const SERVER_NAME = "oxidar-snake-mcp";
const SERVER_VERSION = "0.1.0";

function buildServer(): McpServer {
  const server = new McpServer({
    name: SERVER_NAME,
    version: SERVER_VERSION,
  });

  registerTools(server);

  return server;
}

const CORS_HEADERS: HeadersInit = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, DELETE",
  "Access-Control-Allow-Headers": "Content-Type, Mcp-Session-Id",
};

export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    // Pre-flight — required for browser-based MCP clients
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    // Only serve the /mcp endpoint. Return 404 for everything else
    // (including /.well-known/oauth-authorization-server) so clients
    // know this server requires no authentication.
    if (url.pathname !== "/mcp") {
      return new Response("Not Found", { status: 404, headers: CORS_HEADERS });
    }

    // Ensure Accept header includes both types the MCP SDK requires.
    // Some clients (e.g. Claude Code) may not send the exact header,
    // causing a 406 rejection from the transport layer.
    const reqHeaders = new Headers(request.headers);
    const accept = reqHeaders.get("Accept") ?? "";
    const needs = ["application/json", "text/event-stream"];
    const missing = needs.filter((t) => !accept.includes(t));
    if (missing.length > 0) {
      reqHeaders.set("Accept", [accept, ...missing].filter(Boolean).join(", "));
    }
    const normalizedRequest = new Request(request.url, {
      method: request.method,
      headers: reqHeaders,
      body: request.body,
    });

    const server = buildServer();
    // sessionIdGenerator: undefined → stateless mode; safe for Cloudflare Workers
    // where each invocation is isolated with no shared in-memory state.
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: undefined,
    });

    await server.connect(transport);
    const response = await transport.handleRequest(normalizedRequest);
    // Do NOT call server.close() here — it kills the SSE stream before
    // the client can read it. Workers runtime handles cleanup when the
    // response stream ends.

    // Propagate CORS headers onto every MCP response
    const headers = new Headers(response.headers);
    for (const [k, v] of Object.entries(CORS_HEADERS)) {
      headers.set(k, v);
    }

    return new Response(response.body, {
      status: response.status,
      headers,
    });
  },
};
