---
title: Features
description: The surfaces a project can hold, and the switches that decide which of them are shown.
---

Cydonia ships with most of itself switched off. What a fresh install shows is
articles and boards; the rest appears once you ask for it, in **Settings ›
Features** or in `~/.config/cydonia/settings.toml`:

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

Articles have no switch. They are the one thing the app is for.

`sessions` gates more than a pane. A session is the only thing that starts an
agent, and an agent is a package this machine downloads and runs, so the switch
is the gate over that too.

Turning a feature off hides it. Nothing on disk is deleted, and turning it back
on shows what was there.
