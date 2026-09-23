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

## The rail

A session pane wide enough to leave a gutter beside the text carries a column
of marks down its left edge, one per turn. It answers two things at once: how
much of the session is on screen, and which turn of it you are on.

- The brightest mark is the turn you are reading — the first one on screen,
  counting from the top of the pane. Scrolling to the end does not move it to
  the last turn; it stays on whichever turn heads the screen.
- The marks under it are the other turns on screen. Any part of a turn counts,
  so a prompt half cut off by the top edge is still lit.
- Every other turn in the session is dim.
- Hover a mark for the prompt that opened its turn. Press it to jump there;
  pressing the last one also puts the pane back to following the agent as it
  writes.

A turn behind the composer is not on screen, and its mark stays dim.

The last few turns of a long session cannot be brought to the top of the pane
by scrolling. Press their marks to read them.

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
