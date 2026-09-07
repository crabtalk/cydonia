# Cydonia

https://github.com/user-attachments/assets/dfe51807-a24a-49f0-b702-918c317ee21d

A desktop workspace for the coding agents you run. Open a directory as a
project, put any agent that speaks [ACP](https://agentclientprotocol.com) to work
in it, and keep what comes out as durable artifacts on disk — articles, boards
and tables, not a chat log.

```sh
cargo install cydonia
cydonia
```

> [!NOTE]
> Articles are the stable part, and a fresh install is articles and nothing
> else. Sessions, boards and tables work and are early, so they ship off. One
> agent in one project is solid; several of them working that project is what is
> being built.

## Features

The rest is off until you ask for it, in **Settings › Features** or in
`~/.config/cydonia/settings.toml`:

```toml
[features]
sessions = false   # agent conversations
boards = false     # cards in columns
tables = false     # structured records
```

`sessions` gates agents as much as it gates the pane — a session is the only
thing that starts one, and an agent is a package this machine downloads and
runs. Turning a feature off hides it; nothing on disk is deleted.

## Agents

Install one from the ACP registry in **Settings › Agents**, or write it into
`~/.config/cydonia/settings.toml` yourself:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

## Where things live

```
~/.config/cydonia/   settings.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, sessions and store
```

A project's own store carries a `.gitignore` — none of what cydonia writes
there is the project's source.

## Development

[fixtures](https://github.com/crabtalk/fixtures) is a demo project to open the
app against; `cargo run --example covers -- ../fixtures` gives its articles their
pictures.

## License

[MIT](LICENSE)
