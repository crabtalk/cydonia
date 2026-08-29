# Cydonia

A desktop client for [ACP](https://agentclientprotocol.com) agents — connect any coding agent that speaks the Agent Client Protocol and chat with it.

## Usage

```sh
cargo install --path .
cydonia
```

Cydonia renders streamed markdown, reasoning, tool calls, and the agent's plan; permission requests pop up as a modal; `fs/read_text_file` and `fs/write_text_file` are served to the agent.

| Key | Action |
| --- | --- |
| `Enter` | send (`Shift+Enter` for a newline) |
| `/` | the agent's own slash commands (`Up`/`Down` to pick, `Esc` to dismiss) |
| `Cmd+N` | new session |
| `Cmd+Q` | quit |

## Agents

`~/.config/cydonia/settings.toml` is generated on first run and never needs hand-maintenance — it's seeded with the [Claude Code](https://www.npmjs.com/package/@agentclientprotocol/claude-agent-acp) and [Codex](https://www.npmjs.com/package/@agentclientprotocol/codex-acp) adapters. Any ACP agent is an entry away:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

Set `CYDONIA_DEBUG=/tmp/acp.log` to capture the raw JSON-RPC wire.

Cydonia paints with [bezel](https://github.com/crabtalk/bezel), pinned to 0.1.2 from crates.io.

## Docs

`docs/` is an [mdBook](https://rust-lang.github.io/mdBook/) covering commands, agents, MCP servers, and — worth reading before relying on it — the [limitations](docs/src/limitations.md).

```sh
mdbook serve docs --open
```

## License

[MIT](LICENSE)
