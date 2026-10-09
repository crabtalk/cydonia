---
title: MCP
description: Cydonia as an MCP server, and the MCP servers it offers the agents it runs.
---

Cydonia speaks MCP in both directions: it serves a project's artifacts as tools
an agent can call, and it offers the MCP servers you add to the agents it runs.

## Cydonia as a server

The tool server is on by default and opens nothing until `sessions` is on — the
only caller is an agent, and that switch decides whether any run.

```toml
[mcp]
serve = true
write = false
delete = false
```

`write = false` is a server an agent can read a board through and not change it.
The changing tools are left out of the list rather than refused on the call: a tool an agent can see is one it will spend a turn trying.

`delete` is the narrower of the two and is read after it: `article_remove` and `board_remove` take an entry off the disk with nothing to read back, so they are offered only where both switches are on. Archiving needs `write` alone — an archived entry is listed under the divider rather than gone.

### Connecting a client

The server speaks Streamable HTTP at `http://127.0.0.1:7457/mcp`, on loopback only. When that port is taken, as by a second Cydonia, it takes the next free one up to 7472; **Settings › MCP** shows the address in use.

```sh
claude mcp add --transport http cydonia http://127.0.0.1:7457/mcp
```

There is no token. A request carrying an `Origin` header is refused, so a web page cannot call it.

A client added by hand reaches every project the app has open, and names one per call with the `project` argument, the project directory's path. Sessions Cydonia runs are told their project and may leave it out.

### Tools

| Area | Tools |
| --- | --- |
| Projects | `project_open`, `project_close`, `project_entries`, `project_read_entry` |
| Articles | `article_list`, `article_read`, `article_add`, `article_edit`, `article_rewrite`, `article_rename`, `article_move`, `article_set_cover`, `article_archive`, `article_remove` |
| Boards | `board_list`, `board_search`, `board_read`, `board_add`, `board_rename`, `board_archive`, `board_remove`, `board_add_column`, `board_rename_column`, `board_move_column`, `board_remove_column`, `board_add_card`, `board_rewrite_card`, `board_move_card`, `board_remove_card`, `board_set_card_status` |
| Sessions | `session_send`, `session_read`, `session_search`, `session_rename` |
| Workspace | `workspace_focus` |
| Browser | `browser_tabs`, `browser_open`, `browser_read`, `browser_scroll`, `browser_console`, `browser_click`, `browser_type` |

A board is named by its key (`ROAD`), its name or its id; a card by its handle
(`ROAD-12`) or its id; a column by its name or its id; an article by its title
or its id. The entry references from [Projects and entries](../working/projects.md) work
wherever one of these is taken.

Keys are unique within a project, not across projects. `board_search` takes a key, a handle or a name, or a list of them, and answers every open project holding a matching board; the board tools then take that project.

Eight tools take a list where they take one thing, so a turn that touches several is one call: `project_close` takes several paths, `article_move` several articles, and `board_add_card`, `board_add_column`, `board_remove_card`, `board_remove_column`, `board_move_card` and `board_set_card_status` several cards or columns. Everything else about the call stays singular — one destination, one column, one board. They are all or nothing: every name is resolved before anything is written, so a list with a typo in it changes nothing.

`article_read` and `project_read_entry` take part of an article as well as the whole: `#12:5-7` answers lines 5 to 7, `#12#setup` the section under the heading whose anchor is `setup`, each with the lines it is — see [References](../working/references.md#lines-and-headings). The other article tools take a whole article and refuse a part.

`session_read` and `session_search` take [references](../working/references.md), including a run of turns (`#43:5-7`) and a session in another open project (`cydonia#43`). `session_send` takes a session in the project the call is about (`#43`).

- `session_send` queues a message as another session's next prompt, or starts a new session on a named agent with it. Nothing is waited for or answered back. A message sent from a session starts with `from #42:7`, the sender and the turn it was on.
- `session_read` answers turns of a session, the last 3 when none are named. Each tool call is one line unless `full` is asked for.
- `session_search` finds text case-insensitively in one session or every session in a project, archived ones included, and answers each matching turn as a reference such as `#43:5`, at most 20.
- `session_rename` retitles the session that calls it, and no other. Where you titled the session yourself it is refused unless the agent sets `replace_user_title`, which it is told to do only when you asked for the rename; the new title then replaces yours.

`workspace_focus` answers the articles, boards and tables the window is showing, the focused one marked.

The browser tools drive the tabs in a project's right panel: the user's own browser, signed in where they are. `browser_read`, `browser_click` and `browser_type` work by element numbers from the last read of the tab. `browser_click` and `browser_type` are offered only with `agents_act`, and the rest only with `agents_read`; hosts in `agents_blocked` are refused. See [`[browser]`](../reference/settings.md#browser). A tab's page exists only once its panel has been on screen.

Agents are also offered `markdown` and `browser` resources. [What agents are told](./prompts.md) has their text, and the instructions that come with every prompt.

## Servers you add

**Settings › MCP** installs a server from the registry or takes one you name
yourself, and writes `~/.config/cydonia/mcp.toml`. The settings window owns that
file and rewrites it whole: fields survive the round trip, comments do not.

```toml
[[servers]]
name = "my-server"
enabled = true
command = "npx"
args = ["some-mcp-server@1.2.0"]
# env = { KEY = "VALUE" }

[[servers]]
name = "remote-server"
enabled = true
url = "https://example.com/mcp"
```

A server is either a local `command args...` over stdio or a remote `url`, which
needs the agent to advertise HTTP MCP. A server is enabled unless the file says
otherwise.
