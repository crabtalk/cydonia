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
```

`write = false` is a server an agent can read a board through and not change it.
The changing tools are left out of the list rather than refused on the call: a
tool an agent can see is one it will spend a turn trying.

The server binds a pinned port, so its address survives a restart. Point an
external MCP client at it to reach the same projects the app has open.

### Tools

| Area | Tools |
| --- | --- |
| Projects | `project_open`, `project_close`, `project_entries`, `project_read_entry` |
| Articles | `article_list`, `article_read`, `article_add`, `article_edit`, `article_rewrite`, `article_rename`, `article_move` |
| Boards | `board_list`, `board_read`, `board_add`, `board_add_column`, `board_rename_column`, `board_move_column`, `board_remove_column`, `board_add_card`, `board_rewrite_card`, `board_move_card`, `board_remove_card`, `board_set_card_status` |

A board is named by its key (`ROAD`), its name or its id; a card by its handle
(`ROAD-12`) or its id; a column by its name or its id; an article by its title
or its id. The entry references from [Projects and entries](../working/projects.md) work
wherever one of these is taken.

Agents are also offered a `markdown` resource describing the article syntax, so
a well-behaved one writes what the editor renders.

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
