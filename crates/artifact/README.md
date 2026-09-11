# cydonia-artifact

The artifacts [cydonia](https://cydonia.sh) keeps — articles, boards, sessions
and tables — as the shapes they are, not as the files they happen to be in.
Read a project without linking the app.

```sh
cargo add cydonia-artifact
```

```rust
use cydonia_artifact::project::{Project as _, fs};

let project = fs::Project::new("/path/to/project");

for board in project.boards() {
    println!("{} — {} columns", board.label(), board.columns.len());
}
for record in project.sessions() {
    println!("{} on {} ({})", record.title, record.agent, record.id);
}
```

One module per kind — the same four a project can show — and none of them names
a file. Reading and writing is `project::Project`'s, and `project::fs` is
the one impl cydonia ships — so a board is the same board whichever answered.

| Module | What it is |
| --- | --- |
| `article` | `Article` — title, cover and state; `article::properties` is its file |
| `board` | `Board`, `Column`, `Card` — cards in lanes |
| `session` | `session::record` is what one is filed as, `session::chat` its transcript |
| `table` | `Table`, `ColType`; `table::rows` is `Page`, `Row`, `Edit` |
| `project` | `Project` — what a backend answers; `project::fs` is `.cydonia/` |
| `id` | What names an entry, and keeps naming it |
| `stamp` | The millisecond it was made, and the one it last changed at |

The filesystem is one backend for these, not the definition of them — a board
is a board whether it was read off disk or handed over a connection. Nothing
here reaches for a GUI, which is the rule the crate exists to hold.

Versioned with the app and released alongside it. Pre-1.0: shapes still move.

## License

[MIT](LICENSE)
