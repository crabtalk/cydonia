---
title: Keyboard
description: The default chords, and how to change one.
---

## Defaults

| Command | Chord | Key in `settings.toml` |
| --- | --- | --- |
| Settings… | ⌘, | `open_settings` |
| New Session | ⌘N | `new_session` |
| New Session on Next Agent | ⌥⌘N | `new_session_next` |
| New Article | — | `new_article` |
| New Board | — | `new_board` |
| New Table | — | `new_table` |
| Open Project… | ⌘O | `open_project` |
| Close Project | — | `close_project` |
| Toggle Sidebar | ⌘B | `toggle_sidebar` |
| Toggle Terminal | ⌘J | `toggle_terminal` |
| Toggle Right Panel | ⌘L | `toggle_changes` |
| Open Files | ⌘⇧F | `open_files` |
| Open Review | ⌘⇧G | `open_review` |
| Next Entry | ⌃⇥ | `next_entry` |
| Previous Entry | ⇧⌃⇥ | `prev_entry` |
| Plain Text | ⌘E | `plain_text` |
| Find Card | ⌘F | `find_card` |

The commands without a chord are reachable from the menu. ⌘N is the session, and
three more `New` chords would spend letters you may want for your own. ⌘W closes
a tab in a space.

## Changing one

**Settings › Shortcuts** records a chord rather than taking it typed, so what
lands in the file is what the keyboard actually sends. While a row is recording
the app answers to nothing at all.

The same keys can be written by hand under `[shortcuts]`, using the command key
from the table above:

```toml
[shortcuts]
new_article = "cmd-shift-n"
```

## Activate

One key on that page is not a command. `activate` is registered with macOS and
brings Cydonia forward from inside whatever else you are using:

```toml
[shortcuts]
activate = "alt-space"
```

A key claimed across the whole desktop is one to be asked for, so a fresh
install claims none.

## Emacs movement

```toml
[shortcuts]
emacs = true
```

Adds ⌥B and ⌥F word movement, and ⌥D word deletion, in every text field.
