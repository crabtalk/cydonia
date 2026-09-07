# Cydonia

A desktop workspace for notes and [ACP](https://agentclientprotocol.com) agents.
Open a directory as a project, write articles in it, and hand them to any coding
agent that speaks the Agent Client Protocol.

```sh
cargo install --path .
cydonia
```

## Features

A fresh install is articles and nothing else. The rest is off until you ask for
it, in **Settings › Features** or in `~/.config/cydonia/settings.toml`:

```toml
[features]
sessions = false   # agent conversations
boards = false     # cards in columns
tables = false     # structured records
```

`sessions` gates agents as much as it gates the pane — a session is the only
thing that starts one, and an agent is a package this machine downloads and
runs. Turning a feature off hides it; nothing on disk is deleted.

## Keys

| Key | Action |
| --- | --- |
| `Cmd+O` | open a project |
| `Cmd+N` | new session |
| `Cmd+,` | settings |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | step between entries of the kind on screen |
| `Enter` | send (`Shift+Enter` for a newline) |
| `/` | the agent's own slash commands |

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

The file is generated on first run, seeded with the
[Claude Code](https://www.npmjs.com/package/@agentclientprotocol/claude-agent-acp)
and [Codex](https://www.npmjs.com/package/@agentclientprotocol/codex-acp)
adapters. Entries that name a floating tag like `@latest` are skipped — that is
a different program on every launch.

## MCP servers

Servers offered to every agent go in `~/.config/cydonia/mcp.toml`, by hand:

```toml
[[servers]]
name = "everything"
enabled = true
command = "npx"
args = ["-y", "@modelcontextprotocol/server-everything"]
```

ACP takes the server list only at `session/new`, so edits apply to the next
session rather than the running one. There is no connection status: the protocol
tells a client nothing about whether a server started, so a failed one shows up
as its tools being absent from the agent.

## Where things live

```
~/.config/cydonia/   settings.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, sessions and store
```

A project's own store carries a `.gitignore` — none of what cydonia writes
there is the project's source.

## Development

`cargo run`. Set `CYDONIA_DEBUG=/tmp/acp.log` to capture the raw JSON-RPC wire.

`make bundle` assembles `target/bundle/cydonia.app`, which is what carries the
Dock icon — gpui sets none itself, so a `cargo install` binary stays generic.
The logo is not in the repo: it is downloaded to `assets/icon.png` on the first
bundle, and `make icon` refetches it. An unreachable CDN costs the app its icon,
not its build.

`make dmg` packs it into `target/bundle/cydonia-<version>-<arch>.dmg`. The build
is single-arch, which is why the name says so.

Both are signed ad-hoc, and Gatekeeper rejects ad-hoc. `make release` signs for
real, then notarizes and staples. It is the only target that reaches for a
certificate, and it reads which one from `.env.release` — gitignored, and
holding `APPLE_SIGNING_IDENTITY` and `APPLE_KEYCHAIN_PROFILE`, the latter named
by `xcrun notarytool store-credentials` so no password lands in a file. The
certificate has to be a Developer ID Application; an Apple Development one signs
perfectly well and Gatekeeper refuses it anyway.

Cydonia paints with [bezel](https://github.com/crabtalk/bezel) 0.1.5, built from
a checkout beside this one via the manifest's `[patch.crates-io]`.

[fixtures](https://github.com/crabtalk/fixtures) is a demo project to open the
app against; `cargo run --example covers -- ../fixtures` gives its articles their
pictures.

## License

[MIT](LICENSE)
