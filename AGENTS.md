# Cydonia for agents

Read this before changing anything. It holds the rules every change to this
repository follows, from a person or an agent. `CLAUDE.md` imports it; nothing
else repeats it.

Existing code does not follow every rule here yet: many doc comments argue for
their design. When the code around a change disagrees with this file, follow
this file, and never copy the divergence into new code.

# Pull requests

- Target `main`, one change per pull request.
- A pull request is squash-merged, so its title is the commit: Conventional
  Commits, `type(scope): summary`, one line. No version number in the title.
- The body is a few lines or short bullets: what changed and how it was tested.
- No AI attribution anywhere: no `Co-Authored-By: Claude …`, no "Generated with
  …", no badge or footer, in commits, pull request bodies, comments or issues.
- Never add, edit or delete CI (`.github/workflows/`, `.github/scripts/`,
  anything a CI run executes) in a pull request without asking in an issue
  first.

# Before you push

CI runs these with `RUSTFLAGS=-D warnings`. Run them first:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
```

Run tests with `cargo nextest run` (`cargo install cargo-nextest`), filtered to
the crates and tests the change touches.

A change you could not check says so in the pull request, with what was left
unchecked.

# Docs and comments

Doc comments carry facts and invariants. They never argue that the design is
correct. The next reader, often an agent, takes a doc comment as the whole
truth: a rationalization written once becomes settled fact with nothing left to
contradict it.

- Two tests before a sentence goes in. Would it survive the limitation being
  lifted? "No injection support" survives; it just becomes false and gets
  deleted. "No injection support, because a fence already names the grammar"
  resists the lift by telling the reader it is unnecessary — cut that kind. And
  is the claim checkable against the code without being derivable from it?
  "Compiling this query costs milliseconds and the render loop calls it every
  frame" earns its place: you cannot recover it by reading the function. Scope
  opinions fail this test, and so does paraphrase.
- Never restate what the code says. A paraphrase goes stale the first time
  someone updates the code and not the comment, and a reader who trusts it will
  change working code to match.
- Docs say what code cannot: invariants a caller must uphold, costs that
  motivate a shape, behaviour at a boundary, facts about another system.
- State a boundary flat, with no reason attached.
- Never document a capability that does not exist. A gap is a `TODO` or an
  issue, not prose describing a feature.
- Design rationale and history ("this used to …", "so nothing was ever posted")
  go in the pull request, never in a comment.
- When a limitation is lifted, delete the doc that justified it rather than
  amending it.
- User docs under `docs/` describe what the app does, not why it was designed
  that way.

# Markdown

Markdown files in the repository (`*.md`, `docs/`) are hard-wrapped at 80
columns. Markdown that only GitHub shows — pull request bodies, issues, comments
— is not: one line per paragraph.

# Design

- Before adding an element, check what the surrounding structure already
  provides. Anything that repeats what another layer shows or does is a sign the
  layers are wrong.
- When a fix needs a second fix, or a correction lands on the same surface
  again, the shape is wrong, not the detail. Stop and name the structural
  problem, and what already owns each piece of information or behaviour, in the
  pull request.
- Use the established solution for a problem — the platform's API, the library's
  intended pattern — even when it is the larger change. Never a local
  workaround.
- Stored data is never migrated in place. Read old forms and normalize them on
  input.

# Duplication

- Before writing a helper, lookup or formatter, search the whole workspace for
  it — every crate — by behaviour (the format string, the match, the
  comparison), not only by name.
- When the owner of a type or concept lacks the method you need, add it to the
  owner, never inline at the call site.
- When the helper you need is private to another file, move it somewhere shared
  and widen its visibility. Never write a second copy.
- An operation more than one front end performs belongs in the layer they share.
- When you extract a shared piece, move every existing copy onto it in the same
  change.

# Rust

## Ownership

- Behaviour belongs to the type it acts on: a method, or an associated function
  when it produces the type. A free function is for logic no type owns, and
  lives in the module of the concept it serves. No `utils` or `helpers` modules.
- A conversion that maps to a std trait (`From`, `TryFrom`, `FromStr`,
  `Display`, `Deref`, …) implements the trait.
- Never wrap a field access or an inner method call — make the field `pub` or
  use `Deref`, unless the wrapper changes semantics.
- Plain functions and inherent methods over traits with one implementor; inline
  logic over helpers used once; flat modules over deep nesting. No generics or
  abstractions for future use.

## Imports

- One `use` per crate, nested by module (`use crate::{a::Foo, b::{c, Bar}};`).
  Normalize with `rustfmt +nightly --edition 2024 --config
  imports_granularity=Crate <files>`.
- No empty lines between `use` items; `mod` declarations after them.
- Never `super::` in imports — always `crate::`.
- Import types directly, and functions and consts by module: `use crate::codec;`
  then `codec::sha256_hex(…)`.

## Modules

- A module with submodules lives in `foo/mod.rs`, never `foo.rs` beside a `foo/`
  directory.
- Each file has one focused responsibility.
- A module is either public or a facade over a private module (`mod x; pub use
  x::{Foo, Bar};`), never both. Never `pub use m::*`.

## Tests and dependencies

- Tests live in a `tests/` directory beside `src/`, never in an inline
  `#[cfg(test)] mod tests`.
- Every dependency is declared in the root `[workspace.dependencies]` and
  inherited with `xxx.workspace = true`. Never a version in a member's
  `Cargo.toml`.
- Never a `[patch]` section pointing at a local checkout of bezel or zed.

## Constants

- Magic strings that carry meaning, or where a typo would fail silently, are
  `const`. Format strings and error messages stay inline.

# UI components

Three layers, from general to specific:

- **bezel-ui**: widgets that do not depend on Cydonia's model (row titles, group
  boxes, field labels, toggles, segmented controls, menu cards, popovers, modals
  and dialog cards, text styles). A missing generic widget is a card on the
  bezel DEV board, not a local copy.
- **`crates/gui/src/view/kit/`**: Cydonia pieces used by more than one screen:
  `add` (the "+ Add …" row a list ends with), `dialog` (a dialog over the window
  and its answer buttons), `document` (the editor over an article's or a card's
  directory, with its pictures), `form` (labelled fields), `icon` (entry-kind
  marks), `labels` (the label picker: each view that hangs one holds a `Labels`
  and implements `LabelHost`), `layout` (the window layout picker, flat or
  floating), `link` (links to entries, and the `@` list in `link::mention`),
  `menu` (the rows and triggers every view's `ContextMenu` is built from,
  `MenuHost` over the menu a view holds, and the window's own menu: the launch
  screen's), `new` (the "New …" choices), `orb` (the orb a turn in flight
  wears), `pager` (the dots under a run of pages or images), `rename` (the
  inline name field, and what it renames), `stamp` and `text` (formatters, and
  `CardTitle`), `watch` (the entities a view draws from, observed).
- **`crates/gui/src/view/modals/`**: what goes over the window on its
  `ModalLayer`, raised from more than one screen: `confirm` (the delete
  question), `new_board`, `install_app` (the reference an app is installed
  from), `palette` (search and commands), `desktop` (the desktop-only notice),
  `no_agent` (a session asked for with no agent installed), `settings` (the
  settings sheet, without the `desktop` feature), `whats_new` (the latest
  release's notes, raised once per notes version; `release.rs` holds them and
  the version they are for).
- **`crates/gui/src/view/component/`**: feature pieces a screen hangs in itself
  (terminal, composer, transcript, browser, panels). Shared helpers do not go
  here.
- **Screens** (`crates/gui/src/view/*.rs` and their directories: `board/`,
  `article/`, `library/`, `settings/`): compose the layers above. `view/window/`
  is the window around the panes (root, chrome, menu bar, history, the modal
  layer, `strip` for the window's tabs, `entry` for what the window does to an
  entry, `reach` for how a view acts on the window) and `view/pane/` the `Pane`
  entity in its module root (one per pane of what the window lays out, drawing
  its `Tab`), the `Tab` in `tab.rs`, both drawn `.cached(...)`, and what a tab
  carries (items, find, drawer, footer, its title row in `title`) and how panes
  are arranged. The window's strip (`model/workspace/tabs.rs`) holds a tab per
  entry and per space; under it, the entry in front as a layout of one pane, or
  the space in front's panes. The library and the launch view stand under the
  window's band (`pane/header.rs`) instead. A helper stays private to a screen
  only while that screen is its one user. Pieces shared by one screen's sections
  live in that screen's module root, as `view/settings/mod.rs` holds `Switch`,
  `labelled`, `group` and `lead`.

Rules:

- Before writing a UI piece or a formatter, search bezel-ui, `view/kit/` and the
  screens for it.
- When a second screen needs a piece, move it to `view/kit/` in that change, and
  move every existing copy onto it in the same change.
- Never rebuild by hand what a `view/kit/` or bezel-ui piece does. If it lacks
  an option, add the option.
- Anything over the whole window is a modal: its own entity in `view/modals/`
  (`Focusable`, `EventEmitter<DismissEvent>`, `Render`, drawing its own scrim),
  shown with `Cydonia::toggle_modal` and closed by emitting `DismissEvent`.
  Never an `Option` field on `Cydonia` drawn by its `render`.

# Entities and drawing

- What more than one view draws is an entity in the model: a board is an
  `Entity<Board>` on its `Project` (`HeldBoard`), a session an
  `Entity<ChatSession>` (`HeldSession`), an article's text an `Entity<Document>`
  on its `Article`. A write updates that entity and notifies it, not the
  `Workspace`. The `Workspace` notifies for what the sidebar and the library
  list: an entry made, deleted, renamed, archived or labelled.
- A session's agent pump runs in the session's own context and writes only to
  it.
- A view observes every entity it draws from: `cx.observe` when it is made, or
  `kit::watch::Watch` where which entity it draws can change.
- A piece drawn inside another view from an entity that view does not observe is
  a view of its own, kept by `window.use_keyed_state` and observing its source
  (`board::embed::CardEmbed`, a session excerpt's `SessionView`).
- The sidebar (`sidebar::Sidebar`) and the library (`library::Library`) are
  entities drawn `.cached(...)`. The window hands the sidebar what it shows
  (`sidebar::Lit`) as that changes; the applied search is one `Entity<Search>`
  the window, the sidebar and the panes' find read.
- A view owns its menus and its fields: its own `ContextMenu`, its own rename
  field, its own label picker. The window's is the launch screen's alone. The
  rows of an entry's menu are built once, by `Cydonia::entry_rows`, for the
  `entry::Host` asking.
- `Strip`, `Pane`, `Tab`, `Sidebar` and `Library` are drawn `.cached(...)`. A
  cached view draws again only when it or a view inside it notifies. Notifying
  an entity it read in `render` does not redraw it,
  `window.request_animation_frame()` redraws only the view calling it, and
  `window.refresh()` is never how a change gets on screen.
- A tab's content is one item view per entry, made the first time the tab shows
  it and kept on the `Tab` (`BoardView`, `ArticleView`, `SessionView`,
  `TableView`). What belongs to one pane (scroll, selection, the open card,
  find) is on the item; the content is the model entity it observes.
- An article open in two panes is one `bezel::editor::Document` with one
  `Editor` per pane (`Editor::for_document`). Undo history and anchors are the
  document's; the selection and the typing run are the editor's.
- Find reaches an item through `pane::find::Searchable`, pushed by the tab.
- Updating an entity while it is already being updated panics. A view that acts
  on the window implements `window::reach::InWindow` and goes through
  `defer_window`, and through `InWindow::through` for the window's rows in its
  own menu.
- What the sidebar lists, and in what order, is `sidebar::List`; where a row is,
  what it names and what it is called are methods on `sidebar::Row`.

# Code layout

- Module-level `const` and `static` items go in one block at the top of the
  file, after the `use` lines.
- A file under `view/` belongs to the module that owns it: a piece of one screen
  in that screen's directory, a piece of the window in `view/window/`, of every
  pane in `view/pane/`. Never add a flat sibling to `view/`.
- A concept passed around as `&str` or `&Path` gets a type, and its functions
  become that type's methods (`keymap::Chord`, `update::Version`,
  `mention::Query`, `cover::Seed`). When the type is another crate's, the
  methods go on an extension trait (`kit::text::CardTitle`).
- One interface with an implementation per platform or per build is a trait, one
  struct per implementation, and a type alias naming this build's
  (`update::Platform`, `spaces::Store`, `disk::Files`, `external::Desktop`).
  Never write the same function twice behind `cfg`.
- `model/` never imports from `view/`.
