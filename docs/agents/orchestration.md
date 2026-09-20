---
title: Running several agents
description: More than one agent at a time — dispatching cards, watching runs, and the board that tracks them.
---

One agent in one project is a session. Several of them is what the panes and
the boards are for.

## Starting more than one

⌘N talks to whichever agent the project last used, so it is the same agent
every time. ⌥⌘N starts a session on the next agent installed — that is how a
second one gets a turn while the first is still working.

Sessions run independently. A turn in one does not queue behind a turn in
another, and each reports its own progress.

## Dispatching a card

A card on a board can be handed straight to an agent. Running it opens a
session of its own, in that project, with the card's text as the first prompt.

- The session is the card's own, not the one in front, so the card reports its
  own run — and dispatching a second card does not queue behind the first.
- The card spins an orb while its run is going. It is the same orb the sidebar
  and the transcript use, because it is the same run.
- The board stays up. Clicking the card is what follows the work into the
  transcript.
- A card whose session has been closed is a card you can run again.

Moving a card to another board gives it a new handle and clears the session it
was dispatched to.

## Watching them

| What | Chord |
| --- | --- |
| Step between the panes on screen | ⌃⇥ and ⇧⌃⇥ |
| Close a tab in a space | ⌘W |
| Toggle the sidebar | ⌘B |

Split the window into panes and drag a session into each, or save the
arrangement as a space — one space can span projects. Each pane draws from
its own state, so moving the focus does not move what the others are showing.
See [Panes, tabs and spaces](./../working/spaces.md).

## The board as the status of the work

A card carries the status the work on it is under:

| Status | What it says |
| --- | --- |
| `busy` | Being worked on now |
| `blocked` | Cannot go on |
| `done` | The work is finished |

An agent sets these itself through `board_set_card_status`, and can tag several cards in one write — as it can move several in one write through `board_move_card`. The convention the tools ask for: tag a card `busy` before
starting on it and clear the tag when the turn is over, so a tag left behind
never says an agent is on a card that nobody is.

With a board per project and a status per card, what is running — and what is
stuck — is one screen rather than several transcripts.
