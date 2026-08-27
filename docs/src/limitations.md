# Limitations

What cydonia does not do, and why. Everything here is deliberate or structural — none of it is a bug to report.

## Protocol coverage

Cydonia implements the chat surface of ACP v1: streamed messages and reasoning, tool calls with status and kind, plans, permission requests, session titles, the agent's own slash commands, `fs/read_text_file` and `fs/write_text_file`, cancellation, authentication, and `session/new`.

Not implemented:

| Feature | Status |
| --- | --- |
| `terminal/*` | Declined. Agents fall back to captured command output, so nothing breaks — but long-running commands do not stream live. |
| Elicitation | Declined. Unstable in the protocol. |
| Images and audio | A prompt carries text and embedded resources only. Inbound image and audio blocks render as `[non-text content]`. |
| Diffs | A `ToolCallContent::Diff` is shown as `edited <path>`; the hunks are not rendered. |
| Session modes, config options, usage | Read off `session/new` and `session/update`, but not surfaced. `session/set_mode` and `session/set_config_option` are wired and uncalled. |
| Resume | `session/load` is implemented and never invoked — nothing records the last session id. |
| `session/list`, `session/fork` | Not used. |
| Plan operations, MCP-over-ACP, protocol v2 | Unstable. Skipped until they settle. |
| SSE MCP transport | Only stdio and streamable HTTP are supported. |

Declining is explicit: unhandled session-scoped requests are answered with `method_not_found` rather than left to park, which would hang the agent.

## Fixed at the start of a session

ACP accepts `mcp_servers` only in `session/new`, `session/load`, `session/resume`, and `session/fork` — there is no update method. Edits to `mcp.toml` or `settings.toml` therefore apply to the next session, not the running one.

## Agents

- Agents are configured by hand in `settings.toml`. There is no installer; the ACP registry is not consulted.
- Auth methods are tried in the order the agent advertises them, with no picker. If an agent lists OAuth first and you would rather use an API key, set the key in the environment so the earlier method fails fast, or authenticate with the agent's own CLI first.
- Interactive authentication blocks until you complete it in the browser.

## MCP

- **No connection status.** ACP gives the client no feedback about whether an MCP server started, connected, or failed. Cydonia can show what it *asked* for, not what succeeded. A server that fails to start shows up as its tools being absent from the agent. Clients that own their MCP connections — codex, for instance — can show health because they are the agent; cydonia is not.
- Remote servers are silently dropped for agents that do not support HTTP MCP.
- Servers in `mcp.toml` are global, not per-agent. Every agent you run is offered every enabled server. Per-agent servers go under the agent in `settings.toml`.

## Rendering

- The transcript holds every turn as a live element. There is no virtualization; very long sessions cost more per frame than short ones.
- Streaming updates are coalesced to one notify per 120ms frame rather than one per chunk.
- Transcript history lives in memory and is not persisted. Without resume, restarting loses it.
