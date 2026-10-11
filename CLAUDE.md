# UI components

Three layers, from general to specific:

- **bezel-ui**: widgets that do not depend on Cydonia's model (row titles, group boxes, field labels, toggles, segmented controls, menu cards, popovers, modals and dialog cards, text styles). A missing generic widget is a card on the bezel DEV board, not a local copy.
- **`crates/gui/src/view/kit/`**: Cydonia pieces used by more than one screen: `dialog` (a dialog over the window and its answer buttons), `form` (labelled fields), `icon` (entry-kind marks), `labels` (the label picker), `link` (links to entries, and the `@` list in `link::mention`), `menu` (every screen's `···` and `+` menus, keyed on the window's bezel `ContextMenu`), `new` (the "New …" choices), `stamp` and `text` (formatters, and `CardTitle`).
- **`crates/gui/src/view/modals/`**: what goes over the window on its `ModalLayer`, raised from more than one screen: `confirm` (the delete question), `new_board`, `palette` (search and commands), `desktop` (the desktop-only notice), `settings` (the settings sheet, without the `desktop` feature).
- **`crates/gui/src/view/component/`**: feature pieces a screen hangs in itself (terminal, composer, transcript, browser, panels). Shared helpers do not go here.
- **Screens** (`crates/gui/src/view/*.rs` and their directories: `board/`, `article/`, `library/`, `settings/`): compose the layers above. `view/window/` is the window around the panes (root, chrome, menu bar, history, the modal layer) and `view/pane/` what every pane carries (arrangement, leaf, header, drawer, find). A helper stays private to a screen only while that screen is its one user. Pieces shared by one screen's sections live in that screen's module root, as `view/settings/mod.rs` holds `Switch`, `labelled`, `group` and `lead`.

Rules:

- Before writing a UI piece or a formatter, search bezel-ui, `view/kit/` and the screens for it.
- When a second screen needs a piece, move it to `view/kit/` in that change, and move every existing copy onto it in the same change.
- Never rebuild by hand what a `view/kit/` or bezel-ui piece does. If it lacks an option, add the option.
- Anything over the whole window is a modal: its own entity in `view/modals/` (`Focusable`, `EventEmitter<DismissEvent>`, `Render`, drawing its own scrim), shown with `Cydonia::toggle_modal` and closed by emitting `DismissEvent`. Never an `Option` field on `Cydonia` drawn by its `render`.

# Code layout

- Module-level `const` and `static` items go in one block at the top of the file, after the `use` lines.
- A file under `view/` belongs to the module that owns it: a piece of one screen in that screen's directory, a piece of the window in `view/window/`, of every pane in `view/pane/`. Never add a flat sibling to `view/`.
- A concept passed around as `&str` or `&Path` gets a type, and its functions become that type's methods (`keymap::Chord`, `update::Version`, `mention::Query`, `cover::Seed`). When the type is another crate's, the methods go on an extension trait (`kit::text::CardTitle`).
- One interface with an implementation per platform or per build is a trait, one struct per implementation, and a type alias naming this build's (`update::Platform`, `spaces::Store`, `disk::Files`, `external::Desktop`). Never write the same function twice behind `cfg`.
- `model/` never imports from `view/`.
