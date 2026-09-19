---
title: Settings
description: Every key in settings.toml, and what it does.
---

`~/.config/cydonia/settings.toml` is written with defaults on first run and read
on launch. Everything in it has a control in **Settings** (⌘,); editing the file
by hand is supported, and never required.

`state.toml` beside it is bookkeeping — which projects were open, where the
panels were left. Preferences worth carrying to another machine are in
`settings.toml`; the rest is not.

## Document keys

These come first in the file: a bare key written after a table would belong to
that table.

| Key | Default | What it is |
| --- | --- | --- |
| `cover_memory` | 100 | The ceiling decoded covers run under, in megabytes |
| `watch_bounce` | — | How long a project's `.cydonia/` must go quiet before the watch re-reads it, in milliseconds |
| `auto_update` | `true` | Whether Cydonia looks for a new release on its own |
| `notify_turns` | `true` | Whether a turn finishing in the background is posted to the system |

`auto_update` switches the looking and the fetching, not the swap: a staged
release waits until a restart is asked for. Off is an app that reaches the
network for a version only when the menu item is picked.

`watch_bounce` is clamped on the way in — the ends of the range are what a hand
cannot reach past.

## `[appearance]`

| Key | Default | What it is |
| --- | --- | --- |
| `mode` | system | Light, dark, or whatever the OS is doing |
| `opaque` | unset | Hold the window opaque; unset leaves the frost to the appearance |
| `cursor_blink` | `true` | Off holds the caret lit |
| `text_size` | 13 | The body size the type ladder is scaled against, in points, clamped to 11–17 |
| `article_font_size` | unset | Unset keeps articles following the interface size |
| `terminal_font_size` | — | Points, clamped to 8–40 |
| `file_font_size` | — | Points, clamped to 8–40 |
| `hue` | `0.0` | The greys' oklch hue, in degrees |
| `chroma` | `0.0` | How much of that hue they carry; zero is the shipped neutral |
| `wide_pages` | `false` | How wide a page with nothing of its own to say is set |
| `board_view` | `lanes` | How a new board is laid out; every board then carries its own answer |
| `indent_project_rows` | `true` | Indent sidebar items under project headings |
| `scrollbars` | `scrolling` | `scrolling`, `always` or `never` |
| `sidebar_scrollbars` | `never` | The same three |
| `wrap_code` | `false` | Whether a long line wraps inside a code block rather than scrolling |

A size out of range paints an interface nobody can read the settings window to
fix, so hand-edited sizes are clamped before layout sees them.

## `[features]`

```toml
[features]
sessions = false
boards = true
tables = false
```

See [Features](../general/features.md).

## `[mcp]`

```toml
[mcp]
serve = true
write = false
```

See [MCP](../agents/mcp.md).

## `[shortcuts]`

```toml
[shortcuts]
emacs = false
new_article = "cmd-shift-n"
activate = "alt-space"
```

`emacs = true` adds ⌥B/⌥F word movement and ⌥D word deletion. Every other key is
a command name and the chord it answers to. An unknown command name is kept, so
a file edited by hand is not rewritten under you. See [Keyboard](./keyboard.md).

## `[[agents]]`

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

See [Installing agents](../agents/install.md).
