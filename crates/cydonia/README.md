# cydonia

A desktop workspace for the coding agents you run. Open a directory as a
project, put any agent that speaks [ACP](https://agentclientprotocol.com) to
work in it, and keep what comes out as durable artifacts on disk — articles,
boards and tables, not a chat log.

```sh
cargo install cydonia
cydonia
```

Articles are the stable part, and a fresh install is articles and nothing else.
Sessions, boards and tables ship off; turn them on in **Settings › Features** or
in `~/.config/cydonia/settings.toml`.

In a session, choose **Git changes** from the **+** beside the message input
(⇧⌘G, or **View › Toggle Git Changes**) to review
staged, unstaged and untracked files in its repository. Select a file for its
unified diff. The panel refreshes every three seconds while open and supports
copying the diff. Code wraps to the resizable panel width with vertical-only
scrolling; individual change hunks can be collapsed. These are repository changes shared by sessions using that
working directory.

```
~/.config/cydonia/   settings.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, sessions and store
```

The shapes under `.cydonia/` are [`cydonia-artifact`](https://crates.io/crates/cydonia-artifact),
a crate of their own — read a project without linking the app.

Full documentation, agent setup and screenshots:
**[github.com/crabtalk/cydonia](https://github.com/crabtalk/cydonia)**

## License

[MIT](LICENSE)
