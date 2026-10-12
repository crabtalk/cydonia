# UI components

Three layers, from general to specific:

- **bezel-ui**: widgets that do not depend on Cydonia's model (row titles, group boxes, field labels, toggles, segmented controls, menu cards, popovers, modals and dialog cards, text styles). A missing generic widget is a card on the bezel DEV board, not a local copy.
- **`crates/gui/src/view/kit/`**: Cydonia pieces used by more than one screen: `dialog` (a dialog over the window and its answer buttons), `document` (the editor over an article's or a card's directory, with its pictures), `form` (labelled fields), `icon` (entry-kind marks), `labels` (the label picker: each view that hangs one holds a `Labels` and implements `LabelHost`), `link` (links to entries, and the `@` list in `link::mention`), `menu` (the rows and triggers every view's `ContextMenu` is built from, and the window's own two menus: the band's and the launch screen's), `new` (the "New …" choices), `rename` (the inline name field), `stamp` and `text` (formatters, and `CardTitle`), `watch` (the entities a view draws from, observed).
- **`crates/gui/src/view/modals/`**: what goes over the window on its `ModalLayer`, raised from more than one screen: `confirm` (the delete question), `new_board`, `palette` (search and commands), `desktop` (the desktop-only notice), `settings` (the settings sheet, without the `desktop` feature).
- **`crates/gui/src/view/component/`**: feature pieces a screen hangs in itself (terminal, composer, transcript, browser, panels). Shared helpers do not go here.
- **Screens** (`crates/gui/src/view/*.rs` and their directories: `board/`, `article/`, `library/`, `settings/`): compose the layers above. `view/window/` is the window around the panes (root, chrome, menu bar, history, the modal layer, `entry` for what the window does to an entry, `reach` for how a view acts on the window) and `view/pane/` the `Pane` entity in its module root (one per pane of the window, holding its tab bar and its `Tab`s), the `Tab` in `tab.rs`, both drawn `.cached(...)`, and what a tab carries (items, find, drawer, footer, header) and how panes are arranged. A helper stays private to a screen only while that screen is its one user. Pieces shared by one screen's sections live in that screen's module root, as `view/settings/mod.rs` holds `Switch`, `labelled`, `group` and `lead`.

Rules:

- Before writing a UI piece or a formatter, search bezel-ui, `view/kit/` and the screens for it.
- When a second screen needs a piece, move it to `view/kit/` in that change, and move every existing copy onto it in the same change.
- Never rebuild by hand what a `view/kit/` or bezel-ui piece does. If it lacks an option, add the option.
- Anything over the whole window is a modal: its own entity in `view/modals/` (`Focusable`, `EventEmitter<DismissEvent>`, `Render`, drawing its own scrim), shown with `Cydonia::toggle_modal` and closed by emitting `DismissEvent`. Never an `Option` field on `Cydonia` drawn by its `render`.

# Entities and drawing

- What more than one view draws is an entity in the model: a board is an `Entity<Board>` on its `Project` (`HeldBoard`), a session an `Entity<ChatSession>` (`HeldSession`), an article's text an `Entity<Document>` on its `Article`. A write updates that entity and notifies it, not the `Workspace`. The `Workspace` notifies for what the sidebar and the library list: an entry made, deleted, renamed, archived or labelled.
- A session's agent pump runs in the session's own context and writes only to it.
- A view observes every entity it draws from: `cx.observe` when it is made, or `kit::watch::Watch` where which entity it draws can change.
- A piece drawn inside another view from an entity that view does not observe is a view of its own, kept by `window.use_keyed_state` and observing its source (`board::embed::CardEmbed`, a session excerpt's `SessionView`).
- The sidebar (`sidebar::Sidebar`) and the library (`library::Library`) are entities drawn `.cached(...)`. The window hands the sidebar what it shows (`sidebar::Lit`) as that changes; the applied search is one `Entity<Search>` the window, the sidebar and the panes' find read.
- A view owns its menus and its fields: its own `ContextMenu`, its own rename field, its own label picker. The window's are the band's and the launch screen's alone. The rows of an entry's menu are built once, by `Cydonia::entry_rows`, for the `entry::Host` asking.
- `Pane`, `Tab`, `Sidebar` and `Library` are drawn `.cached(...)`. A cached view draws again only when it or a view inside it notifies. Notifying an entity it read in `render` does not redraw it, `window.request_animation_frame()` redraws only the view calling it, and `window.refresh()` is never how a change gets on screen.
- A tab's content is one item view per entry, made the first time the tab shows it and kept on the `Tab` (`BoardView`, `ArticleView`, `SessionView`, `TableView`). What belongs to one pane (scroll, selection, the open card, find) is on the item; the content is the model entity it observes.
- An article open in two panes is one `bezel::editor::Document` with one `Editor` per pane (`Editor::for_document`). Undo history and anchors are the document's; the selection and the typing run are the editor's.
- Find reaches an item through `pane::find::Searchable`, pushed by the tab.
- Updating an entity while it is already being updated panics. A view that acts on the window implements `window::reach::InWindow` and goes through `defer_window`, and through `InWindow::through` for the window's rows in its own menu.
- What the sidebar lists, and in what order, is `sidebar::List`; where a row is, what it names and what it is called are methods on `sidebar::Row`.

# Code layout

- Module-level `const` and `static` items go in one block at the top of the file, after the `use` lines.
- A file under `view/` belongs to the module that owns it: a piece of one screen in that screen's directory, a piece of the window in `view/window/`, of every pane in `view/pane/`. Never add a flat sibling to `view/`.
- A concept passed around as `&str` or `&Path` gets a type, and its functions become that type's methods (`keymap::Chord`, `update::Version`, `mention::Query`, `cover::Seed`). When the type is another crate's, the methods go on an extension trait (`kit::text::CardTitle`).
- One interface with an implementation per platform or per build is a trait, one struct per implementation, and a type alias naming this build's (`update::Platform`, `spaces::Store`, `disk::Files`, `external::Desktop`). Never write the same function twice behind `cfg`.
- `model/` never imports from `view/`.
