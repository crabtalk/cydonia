---
title: What agents are told
description: The text Cydonia hands an agent alongside your prompt and over MCP, word for word.
---

Cydonia tells an agent about itself in three places. Everything below is the text the agent receives, rendered from the same files the app is built from.

## With every prompt

Each prompt you send a session carries a context block, `cydonia://session/context`, after your own text. It is rebuilt every turn, so it is current for the project and for what is on screen. It is put together from:

1. The workspace introduction.
2. `Current project:` and the project's directory.
3. The artifact rules and the resource catalog, when the MCP server is reachable. When it is not, a line telling the agent the tools and references cannot be loaded.
4. `On screen in cydonia:` and the entries open in the window, when there are any.

| Part | Covers | Source |
| --- | --- | --- |
| Workspace introduction | What Cydonia is, and reading resources before Cydonia formats | [`workspace.md`](https://github.com/crabtalk/cydonia/blob/main/crates/prompts/instructions/workspace.md) |
| Artifact rules | Entries and how to name them, files under `.cydonia/`, cards you work on | [`artifacts.md`](https://github.com/crabtalk/cydonia/blob/main/crates/prompts/instructions/artifacts.md) |

## As MCP server instructions

An MCP client connecting to Cydonia's [server](./mcp.md) receives the workspace introduction, a line on which project the tools act in, the artifact rules and the resource catalog as the server's instructions. Clients differ in whether and where they show these to the model.

## As MCP resources

The catalog lists these by name and description; an agent reads one through `resources/read` when its task needs it. A resource whose surface is switched off in settings leaves the catalog.

| Resource | Covers | Source |
| --- | --- | --- |
| `cydonia://resources/markdown` | Article Markdown, pictures and covers | [`markdown.md`](https://github.com/crabtalk/cydonia/blob/main/crates/prompts/resources/markdown.md) |
| `cydonia://resources/browser` | Driving the in-app browser | [`browser.md`](https://github.com/crabtalk/cydonia/blob/main/crates/prompts/resources/browser.md) |
