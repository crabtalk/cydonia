---
title: Cydonia
description: A desktop workspace for the coding agents you run — articles, boards and tables kept on your own disk.
---

Cydonia is a desktop workspace for the coding agents you run. Open a directory
as a project, put any agent that speaks [ACP](https://agentclientprotocol.com)
to work in it, and keep what comes out as durable artifacts on disk — articles,
boards and tables, not a chat log.

It runs on macOS on Apple silicon, is written in Rust, and has no account and no
sync. Everything it writes is a file in a directory you already have.

## What is in a project

A project is a directory you opened. What you and your agents write lands in
`.cydonia/` inside it, carrying its own `.gitignore`:

- **Articles** — documents with covers, highlighted code and rich links.
- **Boards** — cards in columns, or as a list.
- **Tables** — structured records.
- **Sessions** — the conversations agents work through.

Articles are the stable part, and a fresh install is articles and boards.
Sessions and tables are off until you turn them on in
[Features](./general/features.md).

## Where to start

- [Getting started](./general/getting-started.md) installs the app and opens a first
  project.
- [Sessions](./agents/sessions.md) and [Installing agents](./agents/install.md) put an
  agent to work in one.
- [Settings](./reference/settings.md) is the reference for `settings.toml`.