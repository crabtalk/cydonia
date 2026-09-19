---
title: Sessions
description: Putting an ACP agent to work in a project, and what it leaves behind.
---

A session is a conversation with an agent, working in a project. It is the only
thing that starts an agent, so it is off until `sessions` is turned on in
[Features](../general/features.md).

⌘N starts a session in the focused project, with whichever agent that project
last talked to. ⌥⌘N starts one on the next agent installed.

## Working

- A turn is split into runs of work in the transcript rather than one unbroken
  stream.
- Quote what an agent said to answer a particular line of it.
- A session can work in any project Cydonia has open, not only the one it
  started in.
- Fork a session to take a conversation in a second direction without losing the
  first.
- Attach an image to a message.

## Notifications

When a turn finishes while Cydonia is in the background, the system is told.
A turn you watched finish is one you already know about, so nothing is posted
while the app is in front. Turn it off with `notify_turns` in
[Settings](../reference/settings.md).

## What it leaves behind

An agent's edits land in the window you were writing in. Saved sessions are
entries, numbered in the same sequence as articles and boards — see
[Projects and entries](../working/projects.md).

Through [MCP](./mcp.md), an agent in a session can read and write the
project's articles and boards directly, which is how work ends up as an artifact
rather than as scrollback.
