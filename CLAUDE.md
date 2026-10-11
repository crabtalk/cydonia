# UI components

Three layers, from general to specific:

- **bezel-ui**: widgets that do not depend on Cydonia's model (row titles, group boxes, field labels, toggles, segmented controls, menu cards, popovers, modals and dialog cards, text styles). A missing generic widget is a card on the bezel DEV board, not a local copy.
- **`crates/gui/src/view/kit/`**: Cydonia pieces used by more than one screen: `dialog` (a dialog over the window and its answer buttons), `form` (labelled fields), `icon` (entry-kind marks), `labels` (the label picker), `link` (links to entries, and the `@` list in `link::mention`), `new` (the "New …" choices), `stamp` and `text` (formatters).
- **`crates/gui/src/view/component/`**: feature pieces a screen hangs in itself (terminal, composer, transcript, browser, panels). Shared helpers do not go here.
- **Screens** (`crates/gui/src/view/*.rs`, `view/settings/`, `view/library/`): compose the layers above. A helper stays private to a screen only while that screen is its one user. Pieces shared by one screen's sections live in that screen's module root, as `view/settings/mod.rs` holds `Switch`, `labelled`, `group` and `lead`.

Rules:

- Before writing a UI piece or a formatter, search bezel-ui, `view/kit/` and the screens for it.
- When a second screen needs a piece, move it to `view/kit/` in that change, and move every existing copy onto it in the same change.
- Never rebuild by hand what a `view/kit/` or bezel-ui piece does. If it lacks an option, add the option.
