# MCP servers

ACP carries a list of MCP servers in `session/new`. Cydonia collects that list and hands it to whichever agent you are talking to — the agent makes the connections and exposes the tools.

## `/mcp`

`/mcp` lists your servers with a checkbox each:

- `space` enables or disables a server
- `d` removes it
- `+ add from the registry` searches the [official MCP registry](https://registry.modelcontextprotocol.io) live; `Tab` runs the search, `Enter` adds the highlighted result

npm-packaged servers are installed into `$XDG_DATA_HOME/cydonia/mcp/<id>`. Remote servers need no install — only their URL is stored.

Everything is written to `~/.config/cydonia/mcp.toml` by the picker. You are not expected to edit it.

## What reaches the agent

Only **enabled** servers are sent. Remote (HTTP) servers are additionally dropped for agents that do not advertise the `mcp_capabilities.http` capability, since those agents cannot reach them.

Tool names are namespaced by the agent, not by cydonia. Claude's adapter, for example, exposes a server named `Filesystem` as `mcp__Filesystem__view`.

## Legacy per-agent servers

Servers may also be declared under an agent in `settings.toml`. They are still honoured, and are offered in addition to the store:

```toml
[[agents.mcp_servers]]
name = "everything"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-everything"]
```

The store is the better place — servers there are available to every agent, including ones installed from the registry.

## Caveats

Changes take effect on the next session, and cydonia cannot tell you whether a server actually connected. Both are explained in [Limitations](./limitations.md#mcp).
