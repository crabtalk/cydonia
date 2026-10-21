---
title: Panes, tabs and spaces
description: Splitting the window, carrying entries between panes, and the panels around them.
---

## Panes

The window splits into panes, and entries drag between them. Each pane draws from its own state, so moving the focus does not move what the other panes are showing. ⌃⌥⇥ and ⇧⌃⌥⇥ step between the panes on screen.

## Tabs

An entry dropped on the middle of a pane joins its stack, in front. A pane holding a stack draws a deck at the head of its title row; a click on the deck lists the stack, to bring another entry to the front or move the one in front to a tab of its own. While the focus is in a pane holding a stack, ⌃⇥ and ⇧⌃⇥ step through the stack instead of the window's tabs, and ⌘1–9 still pick the window's tabs.

⌘W closes a tab in a space. Archiving or deleting the entry a tab shows closes its pane and moves the focus to the top-left pane.

## Spaces

A space is an arrangement of entries, saved and listed in the sidebar with its
entries grouped under it. One space can span projects.

## The panels around them

| Panel | Chord | What it holds |
| --- | --- | --- |
| Sidebar | ⌘B | Projects and their entries |
| Terminal | ⌘J | A shell across the width of the window |
| Right panel | ⌘L | Review, Terminal and Files |

The right panel remembers the tab you left it on and the width you dragged it
to, capped at half the column. In a window too narrow to stand it beside a chat
it covers the column instead. It is not offered beside a space, which divides
the room it would stand in.

⌘⇧F opens Files in it, and ⌘⇧G opens Review.

In a terminal, drag to select; a drag held past the top or bottom edge scrolls the scrollback, faster the further past it goes. A double click selects a word and a triple click a row, and dragging after either extends by words or rows.
