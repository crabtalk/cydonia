---
title: Installing agents
description: Adding an ACP agent from the registry, or pointing Cydonia at a command you already have.
---

Cydonia runs any agent that speaks [ACP](https://agentclientprotocol.com). An
agent is a program Cydonia launches and talks to over stdio.

## From the registry

**Settings › Agents** lists the ACP registry. Installing one downloads and
unpacks it under `~/.local/share/`, and writes an entry into `settings.toml`
pointing at the unpacked executable.

## By hand

Write the entry yourself in `~/.config/cydonia/settings.toml`:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

A hand-written entry has no registry id and is never touched by the installer.

## Pinning

`npx`, `bunx` and `pnpx` resolve a package name against the registry on every
launch, so `npx pkg@latest` is a different program each time. Cydonia checks
whether every package an entry names carries an exact version, and says so where
one does not. Pin the version:

```toml
[[agents]]
name = "pinned-agent"
command = "npx"
args = ["some-acp-agent@1.4.2"]
```

## Running one

An installed agent is reachable from a session. ⌘N talks to whichever agent the
project last used; ⌥⌘N starts a session on the next one. See
[Sessions](./sessions.md).
