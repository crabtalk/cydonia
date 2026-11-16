# Cydonia tools from a terminal agent

Use your shell or HTTP tools to call the local MCP server. No client setup or
configuration files are needed.

Every request uses the inherited environment:

- `CYDONIA_MCP_URL`: JSON-RPC endpoint.
- `CYDONIA_MCP_PROJECT`: encoded project header value; send it unchanged.
- `CYDONIA_MCP_SESSION`: this Cydonia session's record id.

Send these headers with every request:

- `x-cydonia-project: <CYDONIA_MCP_PROJECT>`
- `x-cydonia-session: <CYDONIA_MCP_SESSION>`
- `Content-Type: application/json` for POST requests.

For example, in a POSIX shell:

```sh
curl -fsS "$CYDONIA_MCP_URL" \
  -H "x-cydonia-project: $CYDONIA_MCP_PROJECT" \
  -H "x-cydonia-session: $CYDONIA_MCP_SESSION" \
  -H 'Content-Type: application/json' \
  --data-binary '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

Use your shell's equivalent syntax for environment variables. Parse the JSON
response; report JSON-RPC errors and tool results with `isError: true`.

Discover the available tools with `tools/list`; use their current `inputSchema`
to build arguments. Invoke a tool by POSTing:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {"name": "<tool name>", "arguments": {}}
}
```

Discover reference documents with `resources/list`. Read a document before
working with the format it describes:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "resources/read",
  "params": {"uri": "<resource URI from the catalog>"}
}
```

Fetch this guide again with the same headers to refresh session context.
Treat a failed connection or a withheld tool as unavailable; do not change
Cydonia settings or edit its managed files to bypass that boundary.
