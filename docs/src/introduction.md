# Introduction

Cydonia is a terminal client for [ACP](https://agentclientprotocol.com) agents. Any coding agent that speaks the Agent Client Protocol can be connected and chatted with full-screen in your terminal.

```sh
cargo install --path apps/tui
cydonia
```

On launch you pick an agent. Cydonia renders streamed markdown, reasoning, tool calls with diffs, and the agent's plan; permission requests appear as a modal; `fs/read_text_file` and `fs/write_text_file` are served on the agent's behalf.

Nothing needs hand-configuring. Agents and MCP servers are installed and toggled from inside the app — see [Agents](./agents.md) and [MCP servers](./mcp.md).

Worth reading before you rely on it: [Limitations](./limitations.md) — what is deliberately not implemented, and which changes only take effect on the next session or launch.

Set `CYDONIA_DEBUG=/tmp/acp.log` to capture the raw JSON-RPC wire for any session.
