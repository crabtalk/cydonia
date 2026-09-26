This project is a tour of what Cydonia does. Everything in it — this article, the board beside it, the session under it — is the same kind of thing you would make in a project of your own. Click around, drag cards, edit this text. Nothing you do here leaves the tab.

## Boards an agent works

The **Launch** board tracks a small release. Cards carry a handle like `LAUNCH-3`, short enough to say to an agent: *"pick up LAUNCH-3"*. While an agent works a card it tags it — busy, blocked, done — and the orb on the card says so without you asking.

A card handed to a session keeps the link. Open **Plan the launch** in the sidebar to read the conversation behind the cards in DOING.

## Articles that stay

Articles are Markdown documents filed under `.cydonia/articles/`. They take headings, tables, task lists, images and fenced code:

```rust
fn main() {
    println!("an agent can write this article too");
}
```

- [x] Write the tour
- [ ] Open a project of your own

## Sessions in the project

A session starts an agent — Claude Code, Codex, Gemini and others that speak the Agent Client Protocol — in the project's directory. Cydonia serves the agent its own tools over MCP, so it reads and edits these boards and articles as it works, and you watch the changes land.

In this browser demo the agent is a stand-in: start a session with `⌘N` and it will answer, but only the desktop app runs a real one.

## Spaces, terminal, changes

Arrange entries from several projects side by side in a **space**. Open a terminal with `⌘J` and review what an agent changed with `⌘L`. Those need a real directory, so they belong to the desktop app too.
