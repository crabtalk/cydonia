---
title: Cydonia
description: Run coding agents in your projects, several at once, and keep what they produce on your own disk.
---

Cydonia is a desktop workspace for the coding agents you run. Open a directory
as a project, put any agent that speaks [ACP](https://agentclientprotocol.com)
to work in it, and watch several of them work at once — each in its own
session, each in a pane you can see.

What they produce stays: articles, boards and tables written into the project
directory as files, not a chat log that scrolls away.

It runs on macOS on Apple silicon, is written in Rust, and has no account and
no sync.

## Running agents

- A **session** is one agent working in one project. ⌘N starts one, ⌥⌘N starts
  one on the next agent you have installed, and a session can work in any
  project Cydonia has open.
- **Panes, tabs and layouts** put several of those on screen at once, so a
  running agent is something you watch rather than something you poll.
- A **card** on a board can be handed to an agent: it opens a session of its
  own with the card's text as the first prompt, and spins an orb while the run
  is going. Dispatching a second card does not queue behind the first.
- **Cydonia's own MCP tools** let an agent read and write the project's
  articles and boards directly, which is how a turn ends as an artifact rather
  than as scrollback.

## What it leaves behind

Everything lands in `.cydonia/` inside the project, which carries its own
`.gitignore` — none of it is the project's source, and all of it is on your
disk.

- **Articles** — documents with covers, highlighted code and rich links.
- **Boards** — cards in columns or as a list, and the surface agents report
  their work on.
- **Tables** — structured records.

## Where to start

- [Getting started](./general/getting-started.md) installs the app and gets an
  agent running in a project.
- [Sessions](./agents/sessions.md) and [Installing agents](./agents/install.md)
  are the agents themselves.
- [Running several agents](./agents/orchestration.md) is what the panes and the
  boards are for.
- [Settings](./reference/settings.md) is the reference for `settings.toml`.
