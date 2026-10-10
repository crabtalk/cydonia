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

## Labels

Articles, boards and saved sessions carry labels, several each. A label is lowercase, with spaces written as `-`, so `Q3 Plan` and `q3-plan` are one label. Labels are shared by every open project, and a label exists while at least one entry carries it.

In the Library:

- The Labels column shows each entry's labels. Click a cell to add, create or remove them.
- The Labels heading filters the list to entries carrying any of the labels picked.
- With entries selected, Label applies a label to all of them or takes it off. A dash marks a label only some of them carry.
- A label's `···` in the picker renames or deletes it on every entry carrying it. Renaming onto a label already in use merges the two.
- While a project or label filter is on, it shows above the list; click it to take it off.

In the sidebar, a project's menu has Filter by label, which lists only the project's entries carrying one label. The label shows on the project's heading until you click it, and entries cannot be dragged while it is on. Relaunching lists every entry again.

Agents set an entry's labels with `project_label_entry`, and `project_entries` lists them.

## Arranging the sidebar

Entries list where you left them rather than by what was written to last, so an
agent's writes do not reshuffle the sidebar under you.

- Drag a row to arrange a project's entries by hand.
- Pin an entry to the top of its project from the band's menu.
- A new entry stays at the top until it is arranged, below anything pinned.
- Archiving an entry unpins it and takes it out of its space.
- Right-click a project for its menu. The menus of projects and entries reveal the file or folder in the file manager and copy its path.

## Moving between projects

An article can be moved to another project, and a card to another board. A
session can work in any project Cydonia has open, not only the one it started
in.
