---
title: Running several agents
description: More than one agent at a time — sending cards to sessions, watching runs, and the board that tracks them.
---

One agent in one project is a session. Several of them is what the panes and
the boards are for.

## Starting more than one

⌘N talks to whichever agent the project last used, so it is the same agent
every time. ⌥⌘N starts a session on the next agent installed — that is how a
second one gets a turn while the first is still working.

Sessions run independently. A turn in one does not queue behind a turn in
another, and each reports its own progress.

## Sending cards to a session

▶ on a card, or on a column's heading, opens a menu of sessions to send it to: the session that last held the card, then the project's sessions most recently used, then New session.

- What is sent is a link to each card. A column sends its cards that are neither busy nor blocked, under the column's name.
- The session picked is written down on each card as the one that last held it, the way a session that made a card or tagged it busy is.
- The board stays up. A card spins an orb while an agent has it busy; clicking the orb follows the work into the transcript.

## Watching them

| What | Chord |
| --- | --- |
| Step between the panes on screen | ⌃⌥⇥ and ⇧⌃⌥⇥ |
| Close a tab in a space | ⌘W |
| Toggle the sidebar | ⌘B |

Split the window into panes and drag a session into each, or save the
arrangement as a space — one space can span projects. Each pane draws from
its own state, so moving the focus does not move what the others are showing.
See [Panes, tabs and spaces](./../working/spaces.md).

A session shows what it is doing in place of its icon, on its sidebar row and
its tab:

- an orb while a turn runs;
- a ring that breathes while the agent asks for permission;
- a circle with `!` when the last turn failed, until the next message;
- a filled dot when a turn finished while the session was not on screen, until
  it is shown.

A space's tab, and its heading in the sidebar, show the most pressing of its
sessions: asking, then failed, then finished, then running.

## The board as the status of the work

A card's column says where the work is. On top of that, an agent tags a card busy through `board_set_card_busy` while it works on it, and can tag several cards in one write — as it can move several in one write through `board_move_card`. The convention the tools ask for: tag a card busy before starting on it and take the tag off when the turn is over, so a tag left behind never says an agent is on a card that nobody is.

A card that cannot go on until others are finished names them through `board_set_card_blockers`, and shows `blocked` until they are taken off it.

With a board per project, what is running — and what is stuck — is one screen rather than several transcripts.
