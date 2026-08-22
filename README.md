# Cydonia

A terminal client for [ACP](https://agentclientprotocol.com) agents — connect any coding agent that speaks the Agent Client Protocol and chat with it full-screen in your terminal.

```
 ╭────────────────────────────────────────────────────────────────╮
 │ Cydonia — claude — Ctrl+D to exit, /help for keys              │
 │ agent  Claude Agent 0.68.0 · acp v1                            │
 │ mode   auto (Auto, Manual, Accept Edits, Plan Mode, …)         │
 │ cwd    ~/code/cydonia                                          │
 ╰────────────────────────────────────────────────────────────────╯

 fix the flaky test in ci

⏺ Read tests/e2e.rs
  ⎿ 212 lines

⏺ The retry loop races the server startup — pinning the port fixes it.

┌ claude > ──────────────────────────────────── Fix flaky e2e test ─┐
│>                                                                  │
└───────────────────────────────────────────────────────────────────┘
```

## Usage

```sh
cargo install --path apps/tui
cydonia
```

Pick an agent from the selector and chat. Cydonia renders streamed markdown, reasoning, tool calls, and the agent's plan; permission requests pop up as a modal; `fs/read_text_file` and `fs/write_text_file` are served to the agent.

| Key | Action |
| --- | --- |
| `Enter` | send (`Shift+Enter` for a newline) |
| `/` | slash commands, including the agent's own (`Tab` to complete) |
| `Ctrl+C` | cancel the current turn |
| `PageUp` / `PageDown` | scroll the transcript |
| `Ctrl+D` or `/exit` | quit |

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

## Layout

| Crate | Purpose |
| --- | --- |
| `crates/core` | settings + the ACP session (executor-neutral, no UI deps) |
| `crates/registry` | the ACP and MCP registries, and the installer for both |
| `apps/tui` | the terminal frontend (ratatui) |
| `apps/gui` | the desktop frontend (GPUI), on the same core |

`apps/gui` paints with [bezel](https://github.com/crabtalk/bezel) and needs a sibling `../bezel` checkout to build — a path dependency while the two move together.

## Docs

`docs/` is an [mdBook](https://rust-lang.github.io/mdBook/) covering commands, agents, MCP servers, and — worth reading before relying on it — the [limitations](docs/src/limitations.md).

```sh
mdbook serve docs --open
```

## License

[MIT](LICENSE)
