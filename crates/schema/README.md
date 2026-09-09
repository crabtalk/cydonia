# cydonia-schema

The shapes [cydonia](https://cydonia.sh) keeps in a project's `.cydonia/`:
boards, session records, article properties, tables. Read them without linking
the app.

```sh
cargo add cydonia-schema
```

```rust
use cydonia_schema::{board, record};
use std::path::Path;

let project = Path::new("/path/to/project");

for board in board::list(project) {
    println!("{} — {} columns", board.label(), board.columns.len());
}
for (file, session) in record::list(project) {
    println!("{} on {} ({})", session.title, session.agent, file.display());
}
```

| Module | What it is |
| --- | --- |
| `board` | `Board`, `Column`, `Card` — cards in lanes, one file per board |
| `record` | `Record` — an archived session: its agent, its id, its transcript |
| `chat` | `ChatItem` — what a transcript is made of |
| `data` | `Table`, `Column`, `ColType`, `Page` — the project's SQL tables |
| `properties` | An article's `properties.toml`, edited in place |
| `project` | Where `.cydonia/` is, and the millisecond stamp ids carry |

The filesystem is one backend for these, not the definition of them — a board
is a board whether it was read off disk or handed over a connection. Nothing
here reaches for a GUI, which is the rule the crate exists to hold.

Versioned with the app and released alongside it. Pre-1.0: shapes still move.

## License

[MIT](LICENSE)
