# cydonia-schema

The shapes [cydonia](https://cydonia.sh) keeps in a project's `.cydonia/`:
boards, session records, article properties, tables. Read them without linking
the app.

```sh
cargo add cydonia-schema
```

```rust
use cydonia_schema::{board, session};
use std::path::Path;

let project = Path::new("/path/to/project");

for board in board::list(project) {
    println!("{} — {} columns", board.label(), board.columns.len());
}
for (file, record) in session::list(project) {
    println!("{} on {} ({})", record.title, record.agent, file.display());
}
```

One module per kind — the same four a project can show — and `project`, the
directory they all sit inside.

| Module | What it is |
| --- | --- |
| `article` | Where a document sits; `article::properties` is its `properties.toml` |
| `board` | `Board`, `Column`, `Card` — cards in lanes, one file per board |
| `session` | `Record`, an archived session; `session::chat` is its transcript |
| `table` | `Table`, `ColType`; `table::rows` is `Page`, `Row`, `Edit` |
| `project` | Where `.cydonia/` is, and the millisecond stamp ids carry |

The filesystem is one backend for these, not the definition of them — a board
is a board whether it was read off disk or handed over a connection. Nothing
here reaches for a GUI, which is the rule the crate exists to hold.

Versioned with the app and released alongside it. Pre-1.0: shapes still move.

## License

[MIT](LICENSE)
