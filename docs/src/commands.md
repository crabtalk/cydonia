# Commands

## Keys

| Key | Action |
| --- | --- |
| `Enter` | send (`Shift+Enter` for a newline) |
| `/` | the agent's slash commands |
| `Up` / `Down` | move through the command list |
| `Esc` | dismiss the command list |
| `Cmd+N` | new session |
| `Cmd+Q` | quit |

## Slash commands

Typing `/` opens a dropdown of whatever the connected agent advertises. The list is not fixed — it changes with the agent, and with the session, arriving over `session/update` as the agent reports it.

The commands are not documented here because they differ per agent: Claude's `/compact` is not Codex's.

Cydonia does not interpret them. Everything you type is sent to the agent verbatim, so a command is delivered as an ordinary prompt for the agent to act on, and a typo reaches the agent rather than raising an error locally.
