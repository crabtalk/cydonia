//! 0.1.4 — the presentation preferences out of `state.toml`.
//!
//! Up to 0.1.3 the theme, the caret's blink, the type size, the greys' tint
//! and the page width were written into `state.toml` beside the list of open
//! projects. `state.toml` is machine-written bookkeeping full of absolute
//! paths: not worth hand-editing, and not portable to another machine. Those
//! preferences are both. They live in `settings.toml` under `[appearance]`
//! now — see [`crate::model::settings::Appearance`].
//!
//! Retire this once nobody upgrading can still be on 0.1.3 or earlier. See
//! [`super`].

use crate::model::{settings, state};
use toml_edit::DocumentMut;

/// The keys that moved, in the order they are written back out. Named once, so
/// the carry and its test cannot drift apart.
const MOVED: [&str; 8] = [
    "appearance",
    "opaque",
    "cursor_blink",
    "text_size",
    "hue",
    "chroma",
    "wide_pages",
    "wrap_code",
];

/// What the key was called in `state.toml`, where the two disagree.
///
/// `appearance` was a bare key naming light or dark. In `settings.toml` that
/// is the table's own name, so the value inside it is `mode`.
fn renamed(key: &str) -> &str {
    match key {
        "appearance" => "mode",
        key => key,
    }
}

/// Carry the presentation preferences out of `state.toml` into
/// `settings.toml`. Best effort — see [`super::run`].
pub fn run() {
    let (Some(state_path), Ok(settings_path)) = (state::path(), settings::path()) else {
        return;
    };
    let Ok(state_body) = std::fs::read_to_string(&state_path) else {
        return;
    };
    let settings_body = std::fs::read_to_string(&settings_path).unwrap_or_default();
    let (Ok(mut state), Ok(mut settings)) = (
        state_body.parse::<DocumentMut>(),
        settings_body.parse::<DocumentMut>(),
    ) else {
        return;
    };
    if !carry_appearance(&mut state, &mut settings) {
        return;
    }
    // Settings first. Written the other way round, a crash between the two
    // would take the preferences with it; this way the worst case is a
    // `state.toml` still holding keys nothing reads, which the next launch
    // clears.
    if let Some(dir) = settings_path.parent()
        && std::fs::create_dir_all(dir).is_ok()
        && std::fs::write(&settings_path, settings.to_string()).is_ok()
    {
        let _ = std::fs::write(&state_path, state.to_string());
    }
}

/// The move itself, over the two parsed documents. Answers whether anything
/// changed.
///
/// Pure over the documents so it can be tested without a config directory —
/// the file handling above is the part that cannot be.
///
/// A key already in `[appearance]` wins: the settings file is the one a person
/// edits, and a stale copy left in `state.toml` must never overwrite what they
/// wrote. That is also what makes this safe to run on every launch.
pub fn carry_appearance(state: &mut DocumentMut, settings: &mut DocumentMut) -> bool {
    let mut moved = false;
    for key in MOVED {
        let Some(value) = state.get(key).and_then(|held| held.as_value()).cloned() else {
            continue;
        };
        // Made only once something is actually there to put in it, so a
        // `state.toml` with none of these leaves no empty table behind.
        let table = settings["appearance"].or_insert(toml_edit::table());
        let Some(table) = table.as_table_mut() else {
            return false;
        };
        table.set_implicit(false);
        if !table.contains_key(renamed(key)) {
            table[renamed(key)] = toml_edit::value(value);
        }
        state.remove(key);
        moved = true;
    }
    moved
}
