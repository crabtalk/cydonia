//! 0.1.11 — layouts became spaces, on disk as well as on screen, and the
//! right panel became one per working directory.
//!
//! Up to 0.1.10 an arrangement was a layout: the files were written under
//! `~/.config/cydonia/layouts/`, and `state.toml` named the open one under
//! `layout` and their order under `layouts`. The word is `space` now
//! throughout — see [`crate::model::spaces`] — and nothing reads the old
//! names, so a machine upgrading without this would come up having lost every
//! arrangement it had.
//!
//! The second carry-over is a clear rather than a move: up to 0.1.10
//! `right-panels.json` held one panel per session inside each project, and
//! nothing reads that shape. The tabs it named are a scratch record of what
//! was open, so they are dropped rather than folded into the directory's —
//! several sessions in one project would each have a claim on it and no rule
//! picks between them. The dragged width is kept: it is one number for the
//! whole app and was never per session.
//!
//! Retire this once nobody upgrading can still be on 0.1.10 or earlier. See
//! [`super`].

use crate::model::{settings, state};
use toml_edit::DocumentMut;

/// The keys `state.toml` held them under, old name to new.
const KEYS: [(&str, &str); 2] = [("layout", "space"), ("layouts", "spaces")];

/// Move the directory, rename the two keys, and clear the old panels. Best
/// effort — see [`super::run`].
pub fn run() {
    carry_directory();
    clear_session_panels();
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

/// The file the right panel is written to, which up to 0.1.10 was keyed by
/// session.
fn panels_path() -> Option<std::path::PathBuf> {
    settings::dir().ok().map(|dir| dir.join("right-panels.json"))
}

/// Drop `projects` and keep `width`.
///
/// A file already in the new shape is left alone, which is what makes this
/// safe to run on every launch: `projects` is a map of directories to one
/// panel each there, and rewriting it would throw away a panel written since.
fn clear_session_panels() {
    let Some(path) = panels_path() else {
        return;
    };
    let Ok(body) = std::fs::read_to_string(&path) else {
        return;
    };
    let Some(width) = legacy_width(&body) else {
        return;
    };
    let kept = match width {
        Some(width) => format!("{{\"width\":{width}}}"),
        None => "{}".to_string(),
    };
    let _ = std::fs::write(&path, kept);
}

/// The width to keep, for a file still in the old shape. `None` for one this
/// has already run on, or that was never written by 0.1.10.
///
/// Pure over the text so it can be tested without a config directory — the
/// file handling above is the part that cannot be.
pub fn legacy_width(body: &str) -> Option<Option<f64>> {
    let file: serde_json::Value = serde_json::from_str(body).ok()?;
    // One panel per directory has `tabs` at the value; one per session has a
    // map of records in its place.
    let legacy = file
        .get("projects")?
        .as_object()?
        .values()
        .any(|held| held.get("tabs").is_none());
    legacy.then(|| file.get("width").and_then(serde_json::Value::as_f64))
}
