# Introduction

Cydonia is a desktop client for [ACP](https://agentclientprotocol.com) agents. Any coding agent that speaks the Agent Client Protocol can be connected and chatted with.

```sh
cargo install --path .
cydonia
```

Cydonia renders streamed markdown, reasoning, tool calls, and the agent's plan; permission requests appear as a modal; `fs/read_text_file` and `fs/write_text_file` are served on the agent's behalf. Sessions run side by side, picked from the rail.

Agents and MCP servers are configured in `~/.config/cydonia` — see [Agents](./agents.md) and [MCP servers](./mcp.md).

Worth reading before you rely on it: [Limitations](./limitations.md) — what is deliberately not implemented, and which changes only take effect on the next session or launch.

Set `CYDONIA_DEBUG=/tmp/acp.log` to capture the raw JSON-RPC wire for any session.
