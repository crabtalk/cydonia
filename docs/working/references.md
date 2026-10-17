---
title: References
description: How to name an entry, a card, a run of turns in a session, or part of an article, in this project or another.
---

A reference names something in a project in a short form that you and agents can both write.

## Forms

| Reference | Names |
| --- | --- |
| `#43` | Entry 43 in this project |
| `#43:5` | Turn 5 of session #43 |
| `#43:5-7` | Turns 5 to 7 of session #43 |
| `#12:5-7` | Lines 5 to 7 of article #12 |
| `#12#setup` | The section under the heading of article #12 whose anchor is `setup` |
| `DEV-12` | Card DEV-12 in this project |
| `cydonia#43` | Entry 43 in the project `cydonia` |
| `cydonia#43:5-7` | Turns 5 to 7 of session #43 in the project `cydonia` |

Written without a project, a reference means the project you are in.

## As a link

Written after `cydonia://`, a reference is a link: `cydonia://cydonia#43:5-7`. In an article or a session it opens the entry in a drawer at the foot of the pane, or shows it as a chip or a card — see [Articles](./articles.md#links-to-sessions-articles-and-boards). A session's turns can be copied as a link from the session itself — see [Sessions](../agents/sessions.md#linking-turns).

## Entries and cards

`#43` is the number an entry carries — see [Projects and entries](./projects.md). `DEV-12` is a card's handle: its board's key and its number on that board — see [Boards and tables](./boards.md).

## Finding an entry

`@` in an article or a message, and the search palette, list the entries and cards what you type names, in this order:

1. The entry whose number or board key you typed exactly: `#43`, `cydonia#43`, `43`, `DEV`.
2. Cards whose handle starts with what you typed, the exact one first: `DEV-12`, `DEV-1`, `bezel#DEV-12`. A number alone, such as `12`, names this project's cards of that number on every board.
3. Entries whose title or one of whose labels starts with what you typed.
4. Entries whose title or one of whose labels contains it, and entries whose number or board key starts with it: `#4`, `DE`.
5. Sessions whose agent's name contains it.
6. In `@` alone, cards whose text contains it: this project's first, archived boards last.

- Titles, labels and card text are matched in every open project; numbers, keys and handles in this project, or in the one named before the `#`.
- What you type is matched against labels as a label is written, so `Machine Learning` finds `machine-learning`.
- With a `#` in what you typed, only numbers, keys and handles are matched: `#12` names entry 12 and no card.
- Within each step, this project's entries come first, then the most recently changed.
- Archived entries come after every other match, unless named exactly.
- With nothing typed, `@` lists this project's entries.

Start with a kind's prefix to list that kind alone:

| Typed | Lists |
| --- | --- |
| `s:` | Sessions |
| `a:` | Articles |
| `b:` | Boards |

`@s:review` lists the sessions matching `review`. A kind's prefix or a label's lists entries alone, no cards. A prefix comes first and a turn range after a number, so `@s:#43` and `#43:5` do not collide.

`l:` and a label lists the entries carrying that label, in every open project: `l:research` lists them all, `l:research plan` those matching `plan`. It follows a kind's prefix when there is one, as in `a:l:research`.

The search palette also lists the commands that match, and the entries whose text contains what you typed.

## Turns

A session is made of turns. Each turn begins with a message sent to the agent and holds everything the agent did in answer, up to the next message. Turns are counted from 1, in the order they happened.

- `:5` names one turn.
- `:5-7` names turns 5, 6 and 7. Both ends are included.

Turns can only be named on a session. `session_read` and `session_search` take and answer them — see [MCP](../agents/mcp.md).

## Lines and headings

On an article, `:5-7` names lines instead of turns, counted from 1 in its markdown as `article_read` answers it. A line range past the last line stops there.

`#setup` after an article's number names a heading by its anchor, and the section under it: from the heading to the next heading at its level or above.

- An anchor is the heading's text as GitHub writes it: lowercased, punctuation dropped except `-` and `_`, each space a `-`. `## Set up` has the anchor `set-up`, so `#12#set-up` names it.
- A heading whose anchor an earlier heading already has gets `-1`, `-2` and so on.
- Line numbers move when lines are added above them; an anchor moves only when its heading is renamed.

Opening one shows the article with that part at the top of the pane. Lines and headings can only be named on an article. `article_read` and `project_read_entry` answer that part alone, with its line numbers.

## Other projects

Put the project's name in front of the `#` to reach a session in another project: `cydonia#43:5-7`. `session_read` and `session_search` take this form; the other tools take `#43` in the project the call is about.

- The name is the last part of the project's folder: `~/repos/cydonia` is `cydonia`.
- The project must be open in Cydonia.
- When two open projects have the same name, the reference is refused and both paths are listed.
- A folder whose name contains a space, `#` or `:` cannot be named this way.
- Renaming a project's folder breaks references that use its old name.
