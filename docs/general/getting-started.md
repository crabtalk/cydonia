---
title: Getting started
description: Install Cydonia, open a project, and get an agent working in it.
---

## Install

Download the disk image from [cydonia.sh](https://cydonia.sh), or build it from
crates.io:

```sh
cargo install cydonia
cydonia
```

Cydonia runs on macOS on Apple silicon. It asks for no account and reaches the
network only to look for a new release and to install an agent you asked for.

## Open a project

**File › Open Project…** (⌘O) picks a directory — a repository, or anything
else you want an agent working in. Cydonia creates `.cydonia/` inside it the
first time something is written, and that directory carries a `.gitignore`:
none of what Cydonia writes there is your project's source.

Several projects can be open at once, and one session can work in any of them.

## Turn on sessions

A fresh install runs no agents. Open **Settings › Features** (⌘,) and turn on
`sessions`:

```toml
[features]
sessions = true
```

That switch is the gate over every agent this machine would run, so nothing
starts until it is on. See [Features](./features.md).

## Install an agent

**Settings › Agents** lists the ACP registry; installing one unpacks it under
`~/.local/share/` and writes it into `settings.toml`. An agent you already have
can be named there yourself:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
```

See [Installing agents](./../agents/install.md).

## Put it to work

⌘N starts a session in the focused project, with whichever agent that project
last talked to. Type what you want done.

While it works, its edits land in the window you are already looking at. When
the turn finishes with Cydonia in the background, the system says so.

From here:

- [Running several agents](./../agents/orchestration.md) — more than one at
  once, and the board that tracks them.
- [Articles](./../working/articles.md) — what an agent writes into, and what
  you write yourself.
