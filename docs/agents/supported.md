---
title: Supported agents
description: Claude Code, Codex, Cursor, OpenCode, pi and the rest of the ACP registry — what each one installs and runs.
---

Cydonia runs any agent that speaks [ACP](https://agentclientprotocol.com), the Agent Client Protocol. **Settings › Agents** lists every agent in the ACP registry and installs the one you pick. Five of them: An agent signs in and bills the way it does on its own: Cydonia holds no account for it.

An agent the registry ships as an npm package is installed with `npm`, so Node.js has to be on `PATH`. An agent shipped as a binary is downloaded for this machine and unpacked under `~/.local/share/`. See [Installing agents](./install.md).

## Claude Code

Anthropic's Claude Code, through the ACP adapter for the Claude Agent SDK.

| Registry id | Package |
| --- | --- |
| `claude-acp` | npm, `@agentclientprotocol/claude-agent-acp` |

## Codex

OpenAI's Codex, through its ACP adapter.

| Registry id | Package |
| --- | --- |
| `codex-acp` | npm, `@agentclientprotocol/codex-acp` |

## Cursor

Cursor's agent CLI, which speaks ACP itself.

| Registry id | Package |
| --- | --- |
| `cursor` | binary, runs `cursor-agent acp` |

## OpenCode

OpenCode, which speaks ACP itself.

| Registry id | Package |
| --- | --- |
| `opencode` | binary, runs `opencode acp` |

## pi

The pi coding agent, through a community ACP adapter.

| Registry id | Package |
| --- | --- |
| `pi-acp` | npm, `pi-acp` |

## Any other agent

An agent outside the registry, or a build of your own, is an entry in `settings.toml` naming the command that starts it in ACP mode. See [Installing agents](./install.md#by-hand).
