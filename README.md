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

## Windows

Cydonia runs natively on Windows — the same binary, the same files, and the
same agents, with the platform's own conventions where they differ from a
Mac's:

- **Shortcuts** use Ctrl where macOS uses ⌘: Ctrl+N for a session, Ctrl+O
  for a project, Ctrl+, for settings, Ctrl+B for the sidebar, Ctrl+1–4 for
  the panes, Ctrl+Alt+←/→ or Ctrl+Tab between entries, F11 for full screen.
  Ctrl+W closes the window and, with no window left, quits; so does Alt+F4.
- **The window** draws its own minimize, maximize and close buttons at the
  top right. Drag it by the strip at the top of the sidebar; double-click
  there to maximize.
- **Files** live where they do everywhere else, under your profile:
  `%USERPROFILE%\.config\cydonia\` for settings and the catalogue cache,
  `%USERPROFILE%\.local\share\cydonia\` for installed agents.
  `XDG_CONFIG_HOME` and `XDG_DATA_HOME` are honoured if set.
- **Agents** need [Node.js](https://nodejs.org) on `PATH`. The default
  entries say `npx`, which is looked up the way a terminal would — through
  `PATH` and `PATHEXT`, so it finds `npx.cmd` — and started with no console
  window. Every process an agent starts is put in a job object and ends with
  the session, which is the closest Windows comes to a process group.
  Installing from **Settings › Agents** runs `npm.cmd`, and unpacks a binary
  release with the `tar` Windows ships. That half lives in a repo-local patch
  of `cacp-agents`; see [`patches/`](patches/README.md).

To build: [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
with the C++ workload, for the SQLite this crate compiles in, and a current
stable Rust — the pinned gpui uses `slice::as_array`, which 1.90 does not yet
have. Then

```
cargo build --release --bin cydonia
target\release\cydonia.exe
```

A debug build keeps a console window open beside the app, which is where a
panic shows; a release build does not. There is no installer, icon or code
signature yet, and no Windows equivalent of the `make bundle` target.

## Development

[fixtures](https://github.com/crabtalk/fixtures) is a demo project to open the
app against; `cargo run --example covers -- ../fixtures` gives its articles their
pictures.

## License

[MIT](LICENSE)
