Cydonia is a workspace for working with agents in a project directory. This is a project like any other — a folder on your disk, with everything Cydonia keeps for it in a `.cydonia/` directory inside. Nothing here is special; you can delete it once you are done reading.

## A project is a directory

Open a project with `⌘O` and pick any folder — a repository you work in, a directory of notes, anywhere. Everything you make in Cydonia is filed under `.cydonia/` inside that folder, so moving the folder moves the work with it, and a project checked into git carries its boards and articles along.

That directory is also where an agent runs. A session's working directory _is_ the project, which is why the empty pane names the path before anything has been said.

## The three kinds

**Articles** are documents — notes, specs, anything durable. This is one.

**Boards** hold cards in columns. They are for tracking work, and an agent can read and write them while it works.

**Sessions** are conversations with an agent. Each one starts an agent process in the project directory.

## Installing an agent

No agent ships with Cydonia — an agent is a package this machine downloads and runs, so it waits for you to pick one. Press `⌘N` and the pane offers the install; or open settings with `⌘,` and go to **Agents**.

Once one is installed, `⌘N` starts a session in whatever project is in front.

## Worth knowing

| Chord | What it does |
| --- | --- |
| `⌘O` | Open a project |
| `⌘N` | New session |
| `⌘B` | Show or hide the sidebar |
| `⌘J` | Terminal |
| `⌘L` | Changes |
| `⌘,` | Settings |

Preferences live in `~/.config/cydonia/settings.toml` and are meant to be hand-edited. Agents and grammars are downloaded to `~/.local/share/cydonia/`.

The board beside this article has a few things to try. When you want to use Cydonia on real work, open a directory of your own — this project is a demonstration, and your work is better kept where you keep the rest of it.