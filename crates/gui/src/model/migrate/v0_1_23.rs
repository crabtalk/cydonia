//! 0.1.23 — `keep_pasted_images` in `settings.toml` became
//! `download_web_images`.
//!
//! The switch only ever governed a pasted picture's web address; a pasted
//! screenshot or picture file is kept whichever way it is set. Nothing reads
//! the old name, so a machine upgrading without this would come up with the
//! switch back at its default.
//!
//! Retire this once nobody upgrading can still be on 0.1.22 or earlier. See
//! [`super`].

use crate::model::settings;
use toml_edit::DocumentMut;

const FROM: &str = "keep_pasted_images";
const TO: &str = "download_web_images";

/// Rename the key. Best effort — see [`super::run`].
pub fn run() {
    let Ok(path) = settings::path() else {
        return;
    };
    let Some(mut document) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|body| body.parse::<DocumentMut>().ok())
    else {
        return;
    };
    if rename_key(&mut document) {
        super::keep("v0_1_22", &path);
        let _ = std::fs::write(&path, document.to_string());
    }
}

/// The key, over the parsed document. Answers whether anything changed.
///
/// Pure over the document so it can be tested without a config directory.
///
/// A new key already present wins, which is what makes this safe to run on
/// every launch.
pub fn rename_key(settings: &mut DocumentMut) -> bool {
    let Some(value) = settings.get(FROM).and_then(|held| held.as_value()).cloned() else {
        return false;
    };
    settings.remove(FROM);
    if !settings.contains_key(TO) {
        settings[TO] = toml_edit::value(value);
    }
    true
}
