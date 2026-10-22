# Cydonia

[![Crates.io](https://img.shields.io/crates/v/cydonia?logo=rust&logoColor=white)](https://crates.io/crates/cydonia) [![Docs](https://img.shields.io/badge/docs-cydonia.sh-blue)](https://cydonia.sh/docs) [![License](https://img.shields.io/crates/l/cydonia)](LICENSE) [![Discord](https://img.shields.io/discord/1481168707391852659?logo=discord&logoColor=white&label=discord&color=5865F2)](https://discord.gg/yGZDYnwbx6)

https://github.com/user-attachments/assets/ea0f43ce-003a-481a-80df-fcf96c42f6a2

A workspace you share with your agents, on your own disk. Any
[ACP](https://agentclientprotocol.com) agent works in it beside you.

```sh
curl -fsSL https://cydonia.sh/install.sh | sh
```

On Windows, `irm https://cydonia.sh/install.ps1 | iex`. To build from source,
`cargo install cydonia`.

## Where things live

```
~/.config/cydonia/   settings.toml, mcp.toml, the agent catalogue cache
~/.local/share/      installed agents
<project>/.cydonia/  that project's articles, boards, sessions and store
```

A project's own store carries a `.gitignore` — none of what cydonia writes
there is the project's source. More in
[Where things live](https://cydonia.sh/docs/general/files).

## Documentation

- [Getting started](https://cydonia.sh/docs/general/getting-started)
- [Features](https://cydonia.sh/docs/general/features): what to switch on and
  off
- [Installing agents](https://cydonia.sh/docs/agents/install): from the ACP
  registry, or your own
- [References](https://cydonia.sh/docs/working/references): the numbers
  entries go by, and how agents use them

## Development

To send a change, read [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
