# Agents

An agent is a process that speaks ACP over stdio. Cydonia can install them for you, or run one you configure by hand.

## From the registry

The [ACP registry](https://agentclientprotocol.com/get-started/registry) is the protocol's own catalog of agents, published by the same project that publishes the protocol and its SDKs. Each entry pins an exact version, so an agent's build never changes underfoot.

Installing happens in one of two places:

- **The launcher** — "install an agent…" browses the catalog; picking one shows a screen with live installer output and holds on failure.
- **`/agents`** — a checklist of every installable agent. `space` installs an unchecked one or removes a checked one, `u` updates when the registry pins a newer version than you have.

Installs go to `$XDG_DATA_HOME/cydonia/agents/<id>` (`~/.local/share/cydonia` by default) and are recorded so launches are reproducible. The executable is resolved from the installed package's own manifest — nothing resolves packages at chat time.

Installing an agent does not switch the running session. See [Limitations](./limitations.md#fixed-at-startup).

## By hand

`~/.config/cydonia/settings.toml` is generated on first run and seeded with the Claude Code and Codex adapters. Any ACP agent is an entry away:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

Hand-configured agents appear in `/agents` marked `(-)`; they are yours to edit, so the picker will not install or remove them.

## Authentication

When `session/new` comes back with "authentication required", cydonia walks the agent's advertised auth methods in order, calls `authenticate` on each until one succeeds, and retries. Two consequences worth knowing:

- Methods are tried in the agent's order — there is no picker. If an agent lists OAuth first, that is the flow you get.
- Interactive methods block until you finish signing in. The status line says which method is waiting rather than appearing to hang.

Codex authenticates with `CODEX_API_KEY` or `OPENAI_API_KEY` in the environment, or through its ChatGPT flow. Gemini uses a Google sign-in or `GEMINI_API_KEY`.

## Sessions

The last session per agent and working directory is remembered. When one exists, the launcher offers "continue last session", which uses ACP's `session/load` to replay the transcript and restore the agent's context.
