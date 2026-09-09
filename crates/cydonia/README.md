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

```
~/.config/cydonia/   settings.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, sessions and store
```

The shapes under `.cydonia/` are [`cydonia-schema`](https://crates.io/crates/cydonia-schema),
a crate of their own — read a project without linking the app.

Full documentation, agent setup and screenshots:
**[github.com/crabtalk/cydonia](https://github.com/crabtalk/cydonia)**

## License

[MIT](LICENSE)
