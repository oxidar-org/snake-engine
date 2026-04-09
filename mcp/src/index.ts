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
    // Pre-flight — required for browser-based MCP clients
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
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
    await server.close();

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
