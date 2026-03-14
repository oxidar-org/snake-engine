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

    const server = buildServer();
    // sessionIdGenerator: undefined → stateless mode; safe for Cloudflare Workers
    // where each invocation is isolated with no shared in-memory state.
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: undefined,
    });

    await server.connect(transport);
    const response = await transport.handleRequest(request);
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
