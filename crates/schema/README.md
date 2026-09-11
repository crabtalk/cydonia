# cydonia-schema

The shapes [cydonia](https://cydonia.sh) keeps in a project's `.cydonia/`:
boards, session records, article properties, tables. Read them without linking
the app.

```sh
cargo add cydonia-schema
```

```rust
use cydonia_schema::backend::fs;

let project = fs::Project::new("/path/to/project");

for board in project.boards() {
    println!("{} — {} columns", board.label(), board.columns.len());
}
for record in project.sessions() {
    println!("{} on {} ({})", record.title, record.agent, record.id);
}
```

One module per kind — the same four a project can show — and none of them names
a file. Reading and writing belongs to a backend; `backend::fs` is the one
cydonia ships, and a board is the same board whichever one answered.

| Module | What it is |
| --- | --- |
| `article` | `Article` — title, cover and state; `article::properties` is its file |
| `board` | `Board`, `Column`, `Card` — cards in lanes |
| `session` | `Record`, an archived session; `session::chat` is its transcript |
| `table` | `Table`, `ColType`; `table::rows` is `Page`, `Row`, `Edit` |
| `backend` | Where the shapes come from; `backend::fs` is `.cydonia/` |
| `project` | Where `.cydonia/` is |
| `id` | What names an entry, and keeps naming it |
| `stamp` | The millisecond it was made, and the one it last changed at |

The filesystem is one backend for these, not the definition of them — a board
is a board whether it was read off disk or handed over a connection. Nothing
here reaches for a GUI, which is the rule the crate exists to hold.

Versioned with the app and released alongside it. Pre-1.0: shapes still move.

## License

[MIT](LICENSE)
