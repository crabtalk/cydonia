---
title: Sessions
description: One agent working in one project — starting it, watching it, and what it leaves behind.
---

A session is one agent working in one project. It is the only thing that starts
an agent, so it needs `sessions` turned on in [Features](./../general/features.md).

| Chord | What it starts |
| --- | --- |
| ⌘N | A session in the focused project, on the agent it last talked to |
| ⌥⌘N | A session on the next agent installed |

⌘N stays with whoever the project last talked to, so ⌥⌘N is how a second agent
gets a turn. For more than one at a time, see
[Running several agents](./orchestration.md).

## Working

- A turn is split into runs of work in the transcript rather than one unbroken
  stream, so a long turn can be read as it goes.
- Quote what an agent said to answer a particular line of it.
- Attach an image to a message.
- Fork a session to take a conversation in a second direction without losing
  the first.
- A session can work in any project Cydonia has open, not only the one it
  started in.

## Notifications

When a turn finishes while Cydonia is in the background, the system is told. A
turn you watched finish is one you already know about, so nothing is posted
while the app is in front. Turn it off with `notify_turns` in
[Settings](./../reference/settings.md).

## What it leaves behind

An agent's edits land in the window you were writing in. Saved sessions are
entries, numbered in the same sequence as articles and boards — see
[Projects and entries](./../working/projects.md).

Through [MCP](./mcp.md), an agent in a session reads and writes the project's
articles and boards directly. That is how a turn ends as an artifact on disk
rather than as scrollback.
