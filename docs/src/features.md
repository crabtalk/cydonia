# Features

Cydonia holds four kinds of thing in a project: sessions, boards, articles, and tables. Three of them are switched off in a fresh install, so what you get on first launch is articles and nothing else.

Turn them on in **Settings › Features**, or in `~/.config/cydonia/settings.toml`:

```toml
[features]
sessions = false
boards = false
tables = false
```

| Switch | On means |
| --- | --- |
| `sessions` | Sessions can be opened. A session is the only thing that starts an agent, and an agent is a package this machine downloads and runs — so this gates that as much as it gates the pane. |
| `boards` | Cards in columns, one board to a file under the project's `.cydonia/`. |
| `tables` | Structured records in the project's store. |

Articles have no switch. They are what the app is for.

## What "off" means

Off hides a surface. It never touches what is on disk: boards stay in `.cydonia/boards`, transcripts stay in `.cydonia/sessions`, and a table's rows stay in the project's store. A switched-off kind is left out of the sidebar, off the `+` menu, and unreachable by `Ctrl+Tab` — and switching it back on lists everything that was there, with nothing to rebuild.

A project is read whole when it opens, whatever the switches say, so flipping one takes effect immediately.

## Features, not beta

The section is called Features because that is what it is: parts of the app that are optional. It is not a claim that everything under it is unfinished, and rows do not graduate out of it — `sessions` is off because running a downloaded package on your machine is something to ask for, not because it is immature. Where a row *is* early, its own row says so.
