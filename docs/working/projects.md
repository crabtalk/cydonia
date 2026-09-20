---
title: Projects and entries
description: Opening directories as projects, and the numeric references every entry carries.
---

A project is a directory Cydonia has open. ⌘O opens one, and several can be open
at once — they stack in the sidebar, each with its own entries.

## Entries

Articles, boards, tables and saved sessions are entries. Within a project they
share one sequence: `#1`, `#2`, and so on. A reference appears after the title
in a pane header, survives renames and restarts, and is never reassigned after
a deletion through the app.

Agents use the same references. A session or an external MCP client can list a
project's entries with `project_entries` and read one with `project_read_entry`;
the article and board tools accept `#12` wherever they take an entry. See
[MCP](../agents/mcp.md).

## Arranging the sidebar

Entries list where you left them rather than by what was written to last, so an
agent's writes do not reshuffle the sidebar under you.

- Drag a row to arrange a project's entries by hand.
- Pin an entry to the top of its project from the band's menu.
- A new entry stays at the top until it is arranged, below anything pinned.
- Archiving an entry unpins it and takes it out of its space.

## Moving between projects

An article can be moved to another project, and a card to another board. A
session can work in any project Cydonia has open, not only the one it started
in.
