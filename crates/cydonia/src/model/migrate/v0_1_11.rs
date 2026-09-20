//! 0.1.11 — layouts became spaces, on disk as well as on screen.
//!
//! Up to 0.1.10 an arrangement was a layout: the files were written under
//! `~/.config/cydonia/layouts/`, and `state.toml` named the open one under
//! `layout` and their order under `layouts`. The word is `space` now
//! throughout — see [`crate::model::spaces`] — and nothing reads the old
//! names, so a machine upgrading without this would come up having lost every
//! arrangement it had.
//!
//! Retire this once nobody upgrading can still be on 0.1.10 or earlier. See
//! [`super`].

use crate::model::{settings, state};
use toml_edit::DocumentMut;

/// The keys `state.toml` held them under, old name to new.
const KEYS: [(&str, &str); 2] = [("layout", "space"), ("layouts", "spaces")];

/// Move the directory and rename the two keys. Best effort — see
/// [`super::run`].
pub fn run() {
    carry_directory();
    let Some(path) = state::path() else {
        return;
    };
    let Some(mut document) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|body| body.parse::<DocumentMut>().ok())
    else {
        return;
    };
    if rename_keys(&mut document) {
        let _ = std::fs::write(&path, document.to_string());
    }
}

/// `layouts/` becomes `spaces/`, whole.
///
/// A `spaces/` already there wins and the old directory is left alone: that is
/// a machine this has already run on, and a second pass must not put files
/// written since back under names nothing has read for a release.
fn carry_directory() {
    let Ok(dir) = settings::dir() else {
        return;
    };
    let (from, to) = (dir.join("layouts"), dir.join("spaces"));
    if !from.is_dir() || to.exists() {
        return;
    }
    let _ = std::fs::rename(from, to);
}

/// The two keys, over the parsed document. Answers whether anything changed.
///
/// Pure over the document so it can be tested without a config directory —
/// the file handling above is the part that cannot be.
///
/// A new key already present wins, which is what makes this safe to run on
/// every launch.
pub fn rename_keys(state: &mut DocumentMut) -> bool {
    let mut renamed = false;
    for (from, to) in KEYS {
        let Some(value) = state.get(from).and_then(|held| held.as_value()).cloned() else {
            continue;
        };
        state.remove(from);
        if !state.contains_key(to) {
            state[to] = toml_edit::value(value);
        }
        renamed = true;
    }
    renamed
}
