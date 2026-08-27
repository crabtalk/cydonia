# MCP servers

ACP carries a list of MCP servers in `session/new`. Cydonia collects that list and hands it to whichever agent you are talking to — the agent makes the connections and exposes the tools.

## Declaring them

Servers global to every agent go in `~/.config/cydonia/mcp.toml`:

```toml
[[servers]]
name = "everything"
enabled = true
command = "npx"
args = ["-y", "@modelcontextprotocol/server-everything"]

[[servers]]
name = "remote"
enabled = true
url = "https://example.com/mcp"
```

Servers scoped to one agent go under that agent in `settings.toml`:

```toml
[[agents.mcp_servers]]
name = "everything"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-everything"]
```

## What reaches the agent

Only **enabled** servers are sent. Remote (HTTP) servers are additionally dropped for agents that do not advertise the `mcp_capabilities.http` capability, since those agents cannot reach them.

Tool names are namespaced by the agent, not by cydonia. Claude's adapter, for example, exposes a server named `Filesystem` as `mcp__Filesystem__view`.

## Caveats

Changes take effect on the next session, and cydonia cannot tell you whether a server actually connected. Both are explained in [Limitations](./limitations.md#mcp).
