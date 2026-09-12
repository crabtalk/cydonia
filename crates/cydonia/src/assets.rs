//! What `svg()` and `img()` can reach by name: bezel's shipped icons, plus the
//! agent icons [`crate::agent`] has cached.
//!
//! A cached icon's asset path is its own path on disk, so the fallback is a
//! read rather than a table — there is nothing to keep in sync.

use anyhow::Result;
use bezel::gpui::{AssetSource, SharedString};
use std::{borrow::Cow, path::PathBuf};

/// The app's own mark, wherever this build keeps it: beside the binary in a
/// bundle, in the source tree under `cargo run`.
///
/// Nothing at all where neither is there. The logo is downloaded by `make
/// icon` and is not in git, so a fresh clone has none to draw until it has
/// been bundled — and whatever shows it has to hold that case rather than
/// stand an empty box where a picture goes.
pub fn mark() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // `…/cydonia.app/Contents/MacOS/cydonia` — the Makefile puts a small copy
    // of the logo beside the `.icns` that AppKit reads, because nothing here
    // can paint an `.icns`.
    let bundled = exe.parent()?.parent()?.join("Resources").join("icon.png");
    let source = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.png"));
    [bundled, source].into_iter().find(|path| path.is_file())
}

pub struct Assets;

impl AssetSource for Assets {
    /// Files and nothing else. An icon is its own SVG bytes since bezel 0.1.9
    /// — `icons::icon` hands them straight to `svg().data()`, so the set
    /// registers no assets and there is nothing here to ask it for.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(std::fs::read(path).ok().map(Cow::Owned))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
