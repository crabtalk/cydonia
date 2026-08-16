# Commands

## Keys

| Key | Action |
| --- | --- |
| `Enter` | send (`Shift+Enter` for a newline) |
| `/` | slash commands, including the agent's own (`Tab` to complete) |
| `@` | mention a workspace file (`Tab` to complete) |
| `Ctrl+C` | cancel the current turn; twice when idle to quit |
| `PageUp` / `PageDown` | scroll the transcript |
| `Ctrl+D` or `/exit` | quit |

Scrolling up pins the view: the transcript stops following new output until you page back to the bottom.

## Slash commands

Typing `/` opens a dropdown holding two kinds of command: cydonia's own, listed below, and whatever the connected agent advertises. Only the first set is fixed — the rest changes with the agent, and with the session.

### Cydonia's

These seven are always present.

| Command | Action |
| --- | --- |
| `/agents` | install or remove ACP agents from the registry |
| `/mcp` | add and toggle MCP servers |
| `/mode` | list or switch session modes |
| `/config` | list or set config options, including the model |
| `/clear` | clear the transcript |
| `/help` | key and command summary |
| `/exit` | quit |

`/mode` and `/config` are cydonia's commands, but the things they operate on belong to the agent: session modes and config options are advertised at `session/new`, and an agent need not offer either. Against one that offers no modes, `/mode` answers "this agent has no session modes" — the command works, there is simply nothing to switch.

Both take an argument to set, or none to list:

```
/mode plan mode
/config model sonnet
/config fast mode on
```

Both match on either the id or the display name, case-insensitively.

### The agent's

Agents advertise their own commands, and cydonia merges them into completion as they arrive. They are not documented here because they differ per agent — Claude's `/compact` is not Codex's.

Anything cydonia does not recognise is sent to the agent verbatim, so an agent command is delivered as an ordinary prompt for the agent to interpret. A typo therefore reaches the agent rather than raising an error locally.

## Mentions

`@path` embeds a file's contents into the prompt as an ACP resource, so the agent reads it without spending a tool call. Completion is drawn from the git-tracked files in the working directory.

```
Summarize @Cargo.toml in one sentence
```

Mentions require the agent to advertise the `embedded_context` prompt capability. Files over 128 KiB and paths that do not resolve are left as plain text.

## Pastes

Pasting three or more lines (or over 150 characters) collapses to a `[pasted #1 — 5 lines]` placeholder in the composer and the transcript echo. The agent receives the real text.
