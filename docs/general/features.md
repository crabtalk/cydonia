---
title: Features
description: The switches that decide what a project can hold, and what may run in it.
---

Cydonia ships with most of itself switched off. A fresh install shows articles
and boards and runs nothing; the rest appears once you ask for it, in
**Settings › Features** or in `~/.config/cydonia/settings.toml`:

```toml
[features]
sessions = false   # agent conversations
boards = true      # cards in columns
tables = false     # structured records
```

| Feature | Default | What it gates |
| --- | --- | --- |
| `sessions` | off | Agent conversations, and every agent this machine would run |
| `boards` | on | Cards in columns or as a list |
| `tables` | off | Structured records |

**`sessions` is the one that matters.** A session is the only thing that starts
an agent, and an agent is a package this machine downloads and runs, so the
switch is the gate over that as much as over the pane. Nothing runs until it is
on — see [Getting started](./getting-started.md).

Boards are on because a board is files in the project and nothing runs to hold
one.

Articles have no switch.

Turning a feature off hides it. Nothing on disk is deleted, and turning it back
on shows what was there.
