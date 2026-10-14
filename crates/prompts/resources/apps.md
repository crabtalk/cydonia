---
name: apps
description: Write a Cydonia app — a Rust crate in the project's apps/ directory, built to wasm against cydonia-guest, that draws its UI as nodes and keeps data in the project's tables. Use before creating or changing anything under apps/.
---

# Apps

An app is a Rust crate in its own directory under the project's `apps/`:
`apps/<name>/`. Cydonia lists every such directory holding a `Cargo.toml` under
Apps in the sidebar and the library, numbered like any entry, and opens one in a
tab. Opening it builds it; from then on, saving any file of the crate builds it
again and reloads it into its open tabs. A build error, a trap or a panic shows
in the app's tab.

Cydonia builds with

```sh
cargo build --release --target wasm32-unknown-unknown
```

so the machine needs `rustup target add wasm32-unknown-unknown`.

## The crate

```toml
[package]
name = "notes"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
cydonia-guest = "0.2"

[workspace]
```

`cydonia-guest` is the library the app compiles into its `.wasm`; its version
is the version of the Cydonia the app runs in.

```rust
use cydonia_guest::{App, node::*, wire::{Space, TextStyle}};

struct Counter {
    count: u32,
}

#[derive(Clone)]
enum Msg {
    Add,
}

impl App for Counter {
    type Msg = Msg;

    fn init() -> Self {
        Counter { count: 0 }
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Add => self.count += 1,
        }
    }

    fn view(&self) -> Node<Msg> {
        column([
            text(self.count.to_string()).style(TextStyle::Title).into(),
            button("Add").on_click(Msg::Add).into(),
        ])
        .gap(Space::Medium)
        .padding(Space::Large)
        .into()
    }
}

cydonia_guest::app!(Counter, name = "Counter");
```

- Cydonia holds one instance per open tab. The app's state is the `App` value
  in its own memory, kept between calls; `view` is called again after every
  `update`.
- `app!` goes once in the crate root. It writes the exports Cydonia calls and
  the manifest: `name` is what Cydonia lists the app as (the crate's name if
  left out), `calls` the groups of calls the app makes, `hosts` the hosts it
  fetches from:
  `app!(Notes, name = "Notes", calls = [tables])`.
- A call outside the groups the manifest names is answered with an error.

## Nodes

Built with the functions in `cydonia_guest::node`, each turned into a
`Node<Msg>` with `.into()`.

| Node | Built with | Sends |
|---|---|---|
| layout | `column([...])`, `row([...])`, then `.gap`, `.padding`, `.align`, `.justify`, `.grow()`, `.scroll()`, `.surface`, `.width`, `.height`, `.child`, `.children` | — |
| text | `text(s)`, then `.style(TextStyle)`, `.tone(Tone)`, `.lines(n)` | — |
| button | `button(label)`, then `.style(ButtonStyle)`, `.on_click(msg)` | `msg`, cloned |
| input | `input(key)`, then `.placeholder`, `.on_input(Msg::Text)` | the text, on each committed change |
| select | `select(key, [(value, label), ...])`, then `.placeholder`, `.selected(Some(value))`, `.on_select(Msg::Picked)` | the chosen value |
| link | `link(label, url)` | — (Cydonia opens the URL) |
| image | `image(url, width, height)` or `image_bytes(mime, bytes, width, height)`, then `.round()` | — |
| list | `list(key, count, row_height)` | — |

- Styling is by theme tokens only: `Space`, `TextStyle`, `Tone`, `Surface`,
  `ButtonStyle`, `Align`, `Justify` in `cydonia_guest::wire`. There are no
  colours or fonts to set.
- The root node fills the tab. A layout takes the room its parent leaves along
  the parent's axis with `.grow()`.
- A button with no `on_click` is drawn disabled.
- Keys name a node across draws. An input's and a select's key is required: it
  is what Cydonia holds the field's text and the select's menu by.
- `list` is virtualized: it draws `count` rows of `row_height`, filling its
  parent, and Cydonia asks the app for the rows on screen through
  `App::rows(&self, list, range)`, by the list's key. Rows are asked again after
  every `update`.
- `image` from a URL is fetched by Cydonia over `http` or `https`.

## Inputs

Cydonia owns an input's text, caret, selection and composition. The app hears a
committed change through `on_input` — never text still being composed — and
sets the text with `cydonia_guest::host::set_input(key, value)`, which
`on_input` does not hear.

## Tables

With `calls = [tables]` in the manifest, the app reads and writes the project's
tables: the same tables the table view and agents see. Each call answers later,
as the message its last argument makes of the result:

```rust
use cydonia_guest::{tables, wire::ColType};

tables::list(Msg::Tables);                                         // Result<Vec<Table>, String>
tables::create("Notes", [("text", ColType::Text)], Msg::Created);  // Result<Table, String>
tables::insert("notes", rows, Msg::Inserted);                      // Result<Vec<i64>, String>
tables::query("SELECT text FROM notes", Msg::Loaded);              // Result<Rows, String>
tables::delete("notes", "text", "old", Msg::Deleted);              // Result<(), String>
```

- A table's `key` is what SQL names it by, derived from its name at creation;
  its `name` is what Cydonia shows. Look a table up by name with `tables::list`
  rather than creating it on every run.
- `insert` takes `tables::Record`s, column name to JSON value, and writes all of
  them or none.
- `query` runs one read-only statement.
- `delete` takes away the rows whose column holds the value.
- Column types: `Text`, `Number`, `Date` (unix seconds), `Check` (0 or 1).

## Not there

No filesystem, no clock, no network beyond what is listed here.
