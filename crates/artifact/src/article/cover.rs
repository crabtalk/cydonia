//! Where an article's cover sits, and what it is called.
//!
//! In the article's `assets/`, beside the pictures its body points at. One
//! article has at most one, and moving the article carries it without
//! rewriting anything. Covers filed by earlier releases sit in the article's own
//! directory, beside `content.md`, and are still read from there.
//!
//! The name is `cover-<stem>.<ext>`. Only the [`MARK`] is read: whatever
//! follows it is the writer's, and the app puts a number there so shuffling a
//! generated cover lands on a new file rather than overwriting the last.
//!
//! Naming lives here, apart from the app's picture cutting and importing,
//! because a surface that files a cover needs the convention and none of the
//! image handling — see `cydonia::model::cover`.

use std::path::{Path, PathBuf};

/// What every cover file's name starts with.
pub const MARK: &str = "cover-";

/// The proportion a cover is drawn at, as width over height. A picture filed at
/// another shape is not cropped to it: it keeps its own, and stands taller or
/// shorter than the band the app draws.
pub const RATIO: (u32, u32) = (5, 2);

/// The widest a cover is kept at. The app resamples anything wider down to
/// this on the way in.
pub const WIDTH: u32 = 1500;

/// The article's cover, where it has one: in its `assets/`, or else in the
/// article's own directory.
///
/// Takes the path of the `content.md`. The first file a directory offers
/// under [`MARK`] wins — there is only ever one, and a directory holding two
/// is a directory something else wrote into.
pub fn of(content: &Path) -> Option<PathBuf> {
    within(&super::assets(content)).or_else(|| within(content.parent()?))
}

fn within(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| is_cover(path))
}

/// Whether a path names a cover file.
pub fn is_cover(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(MARK))
}

/// Where a cover with this stem and extension goes, in the `assets/` beside
/// `content`. The directory may not exist yet; the writer makes it.
pub fn path(content: &Path, stem: impl std::fmt::Display, ext: &str) -> PathBuf {
    super::assets(content).join(format!("{MARK}{stem}.{ext}"))
}
