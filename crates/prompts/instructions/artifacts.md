## Entries

- Project entries have stable numeric references such as #12, scoped to the current project.
- Use project_entries to discover them and project_read_entry to read one. Article and board tools also accept #12.
- `#12`, `foo#12` and the link `cydonia://foo#12` are Cydonia entry references, not GitHub issues or pull requests. `foo` is the directory name of an open project; without it the entry is in the current project. Read one with project_read_entry.
- `#43:5-7` names turns 5 to 7 of session #43. Read them with session_read, which takes every form above.
- `#12:5-7` names lines 5 to 7 of article #12, and `#12#setup` the section under its heading whose anchor is `setup` (GitHub's anchor: lowercased, punctuation dropped, spaces as `-`). Read either with article_read.
- An entry's link is `cydonia://<project>#<number>`, with `:<turns>` after a session's number for a run of its turns, and `:<lines>` or `#<anchor>` after an article's number for part of it. A board's card is `cydonia://<project>#<handle>`, `cydonia://foo#ROAD-12`, never its board's number. `<project>` is the directory name of the entry's project.
- Replies are Cydonia Markdown. Write an entry named in one as a chip of its link, as the markdown resource's "Links to entries" spells it.
- Name a board by its key (ROAD), its name or its id; a card by its handle (ROAD-12) or its id; a column by its name or its id; an article by its title or its id.
- A board key or handle not in this project: board_search finds the open project it is in.

## Files under .cydonia/

- Use Cydonia tools for managed artifacts under .cydonia/.
- Exception: agents with filesystem access may create an article's own assets/ directory and read or write media files there. Article read and creation results include assets_path, that directory relative to the project directory.
- In assets/, use unique filenames and preserve existing files unless replacement or removal is requested.
- This exception does not override read-only settings or filesystem permissions, and does not grant filesystem access to remote clients.

## Git worktrees

- Check out a git worktree at .cydonia/worktrees/<name> in the project directory, unless the user names another location.
- Reuse a worktree already there for the same work rather than adding a second one.

## Cards you work on

A request that names a card is a card you are working.

- Call board_set_card_status with busy before the first thing you do about it, and clear it with none when you answer.
- This holds however small the request and however often one card comes back: a correction to work already done is that card again, and so is a question about it.
- Use blocked or done where one of those is the lasting answer. A tag left behind says an agent is on a card that nobody is.
- A request naming several cards names them in one call: the card argument takes a list.
