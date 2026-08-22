# Limitations

What cydonia does not do, and why. Everything here is deliberate or structural — none of it is a bug to report.

## Protocol coverage

Cydonia implements the stable chat surface of ACP v1: streamed messages and reasoning, tool calls with status, diffs, kinds and locations, plans, permission requests, session modes, config options, context and cost usage, session titles, the agent's own slash commands, `fs/read_text_file` and `fs/write_text_file`, cancellation, authentication, and `session/new` plus `session/load`.

Not implemented:

| Feature | Status |
| --- | --- |
| `terminal/*` | Declined. Agents fall back to captured command output, so nothing breaks — but long-running commands do not stream live. |
| Elicitation | Declined. Unstable in the protocol. |
| Images and audio | A prompt carries text and embedded resources only. Inbound image and audio blocks render as `[non-text content]`; a terminal cannot do better. |
| `session/list`, `session/fork` | Not used. Resume is keyed on a locally-stored session id instead. |
| Plan operations, MCP-over-ACP, protocol v2 | Unstable. Skipped until they settle. |
| SSE MCP transport | Only stdio and streamable HTTP are supported. |

Declining is explicit: unhandled session-scoped requests are answered with `method_not_found` rather than left to park, which would hang the agent.

## Fixed at startup

Two things cannot change while a session is running.

**MCP servers.** ACP accepts `mcp_servers` only in `session/new`, `session/load`, `session/resume`, and `session/fork` — there is no update method. `/mcp` therefore writes your changes immediately but they apply to the next session, and the picker says so.

**The agent.** Which agent a session talks to is decided when cydonia launches. `/agents` installs and removes take effect on the next launch. This is structural: the ACP connection lives inside a scoped closure that owns the application's lifetime, so swapping agents means restarting that scope.

## Agents and installing

- Only npm-distributed agents install. Of the entries in the ACP registry, roughly half ship as platform binaries or Python packages; those are listed but marked unsupported. Adding them needs archive download, checksum verification, and extraction.
- `npm` must be on `PATH`.
- Auth methods are tried in the order the agent advertises them, with no picker. If an agent lists OAuth first and you would rather use an API key, set the key in the environment so the earlier method fails fast, or authenticate with the agent's own CLI first.
- Interactive authentication blocks until you complete it in the browser.

## MCP

- **No connection status.** ACP gives the client no feedback about whether an MCP server started, connected, or failed. Cydonia can show what it *asked* for, not what succeeded. A server that fails to start shows up as its tools being absent from the agent. Clients that own their MCP connections — codex, for instance — can show health because they are the agent; cydonia is not.
- Remote servers are silently dropped for agents that do not support HTTP MCP. The picker does not yet label them as unavailable for the current agent.
- Only npm (stdio) and streamable-HTTP registry entries can be added; Python and container distributions are listed as unsupported.
- Servers are global, not per-agent. Every agent you run is offered every enabled server.

## Rendering

- The transcript is flattened to lines once per drawn frame. Markdown is parsed once per cell and cached by width, so the cost is clones rather than parsing — but it is still proportional to the number of entries. There is no virtualization; very long sessions cost more per frame than short ones.
- Tabs are expanded and other control characters stripped when content enters the buffer. A literal tab advances the terminal to a tab stop while the layout counts it as zero width, which desynchronizes every cell to its right.
- Code blocks are not syntax-highlighted.
- Transcript history lives in memory. Restarting relies on the agent replaying it through `session/load`.

## The desktop app

`apps/gui` is a working desktop frontend on the same core — streaming, tool cards, permissions, multi-session — but it trails the terminal client. It does not yet have session modes, usage, diffs, config options, mentions, resume, `/mcp`, or `/agents`. Those belong in shared components rather than being written twice.
