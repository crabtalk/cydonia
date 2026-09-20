---
title: Where things live
description: The three directories Cydonia reads and writes.
---

```
~/.config/cydonia/   settings.toml, state.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, tables, sessions and store
```

## The project store

`<project>/.cydonia/` holds everything written about that project, and carries a
`.gitignore` of its own — none of it is the project's source. Copy the directory
and the project's articles and boards come with it.

## Configuration

`settings.toml` is preferences: appearance, shortcuts, features, agents, the MCP
server. It is written with defaults on first run, and hand-editing it is
supported — see [Settings](../reference/settings.md).

`state.toml` is bookkeeping: which projects were open, where the panels were
left. It answers questions only this machine can answer, so it is not worth
carrying to another one.

`mcp.toml` holds the external MCP servers a session can reach. See
[MCP](../agents/mcp.md).

## Installed agents

An agent installed from the registry is unpacked under `~/.local/share/`, and
its `settings.toml` entry points at the unpacked executable. An agent you wrote
into the file yourself is never touched by the installer.
