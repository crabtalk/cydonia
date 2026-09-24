---
title: Boards and tables
description: The surface agents report their work on, and structured records.
---

## Boards

A board is cards in columns. **File › New Board** makes one, and a board is
files in the project — nothing runs to hold one, which is why boards are on in
a fresh install while sessions are not.

A card is a unit of work, and it can be handed to an agent: running it opens a
session of its own with the card's text as the first prompt. See
[Running several agents](./../agents/orchestration.md).

- Switch a board between lanes and a list from its foot. Every board carries
  its own answer; **Settings › Appearance** seeds new ones and does not steer
  boards that exist.
- ⌘F finds a card by typing, and jumps to the one you meant.
- Click a card in lanes or a list to read its full rendered content in the bottom drawer.
  Long lane previews end with **…**. Drag the drawer's top edge to resize it, or
  expand it to fill the pane; Restore returns to the previous height.
  Horizontal swipes move the lanes, including over code and table previews.
- List rows show plain titles derived from the card's Markdown. Click a group
  heading to fold it, use **+** to add a card, or choose Rename from **…**.
- The pencil edits in the drawer; the eye previews your draft. Enter inserts a
  newline, and **✓** or ⌘Enter saves. The undo arrow cancels edits.
  Switching cards or closing with **×** or Escape keeps unsaved drafts in the
  current pane for this app session. Save before closing the pane or quitting.
- An agent lays a board out in one call per kind: the tools that add or drop cards and columns take a list where they take one.
- A card can be moved to another column, another board, or a board in another project. Moving it to another board gives it a new handle and clears the session it was dispatched to. An agent can move several cards in one write, and they may come off different boards.

### Card status

A card carries `busy`, `blocked` or `done`, and an orb while its run is going.
An agent sets the status through the board tools and can tag several cards in
one write; you can set it by hand from the card. Starting work records the
session alongside the busy status. Hover the busy orb to see the linked
session’s name, or click it to open that session.

Status is how a board says what is being worked right now — see
[Running several agents](./../agents/orchestration.md) for what that buys with
more than one agent running.

## Tables

A table is structured records, off until you turn on `tables` in
[Features](./../general/features.md). An agent reading a table is shown a
preview of up to 200 rows.
