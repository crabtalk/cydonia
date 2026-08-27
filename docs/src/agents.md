# Agents

An agent is a process that speaks ACP over stdio.

## Configuring one

`~/.config/cydonia/settings.toml` is generated on first run and seeded with the Claude Code and Codex adapters. Any ACP agent is an entry away:

```toml
[[agents]]
name = "my-agent"
command = "path/to/agent"
args = ["--acp"]
# env = { KEY = "VALUE" }
```

Every configured agent appears in the sessions rail; `Cmd+N` opens a session against one.

## Authentication

When `session/new` comes back with "authentication required", cydonia walks the agent's advertised auth methods in order, calls `authenticate` on each until one succeeds, and retries. Two consequences worth knowing:

- Methods are tried in the agent's order — there is no picker. If an agent lists OAuth first, that is the flow you get.
- Interactive methods block until you finish signing in.

Codex authenticates with `CODEX_API_KEY` or `OPENAI_API_KEY` in the environment, or through its ChatGPT flow. Gemini uses a Google sign-in or `GEMINI_API_KEY`.
