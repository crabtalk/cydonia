---
title: References
description: How to name an entry, a card, or a run of turns in a session, in this project or another.
---

A reference names something in a project in a short form that you and agents can both write.

## Forms

| Reference | Names |
| --- | --- |
| `#43` | Entry 43 in this project |
| `#43:5` | Turn 5 of session #43 |
| `#43:5-7` | Turns 5 to 7 of session #43 |
| `DEV-12` | Card DEV-12 in this project |
| `cydonia#43` | Entry 43 in the project `cydonia` |
| `cydonia#43:5-7` | Turns 5 to 7 of session #43 in the project `cydonia` |

Written without a project, a reference means the project you are in.

## Entries and cards

`#43` is the number an entry carries — see [Projects and entries](./projects.md). `DEV-12` is a card's handle: its board's key and its number on that board — see [Boards and tables](./boards.md).

## Turns

A session is made of turns. Each turn begins with a message sent to the agent and holds everything the agent did in answer, up to the next message. Turns are counted from 1, in the order they happened.

- `:5` names one turn.
- `:5-7` names turns 5, 6 and 7. Both ends are included.

Turns can only be named on a session. `session_read` and `session_search` take and answer them — see [MCP](../agents/mcp.md).

## Other projects

Put the project's name in front of the `#` to reach a session in another project: `cydonia#43:5-7`. `session_read` and `session_search` take this form; the other tools take `#43` in the project the call is about.

- The name is the last part of the project's folder: `~/repos/cydonia` is `cydonia`.
- The project must be open in Cydonia.
- When two open projects have the same name, the reference is refused and both paths are listed.
- A folder whose name contains a space, `#` or `:` cannot be named this way.
- Renaming a project's folder breaks references that use its old name.
