//! The app's own art, by path. There is no `AssetSource`: every picture cydonia
//! paints is a file on disk — a cover, a cached agent mark, the logo — and both
//! `img(PathBuf)` and `Icon::file` read one directly.

use std::path::PathBuf;

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
