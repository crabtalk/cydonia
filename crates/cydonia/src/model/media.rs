//! Pictures a document points at: where a pasted screenshot's bytes go.
//!
//! A document holds a URL, and bytes off the clipboard have no address
//! anywhere — so somewhere has to be picked before an image block can exist,
//! which is what `editor::set_image_store` asks the app. The answer is the
//! article's own directory: the directory *is* the article (see
//! [`crate::model::article`]), so a picture in it is carried by the document it
//! belongs to and deleted with it.
//!
//! The editor names which document is asking, but what it hands over is an
//! `Entity<Editor>` and the article behind one is the workspace's to know —
//! so which directory to write into is still noted by [`aim`] as a document
//! opens. One window and one document in front of it, so one target.

use bezel::gpui::{App, Entity, hash};
use editor::{Editor, ImageStore, Source};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// What a picture's file name begins with, beside the `cover-` it may sit next
/// to — see [`crate::model::cover`].
const MARK: &str = "media-";

/// Where the open document's pictures go. Nothing aimed is a paste the editor
/// lets go of, which is what a screenshot pasted with no document open is.
static TARGET: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Install the store. Called once, beside the other `init`s.
///
/// `accepts` is left at the editor's own guess from the extension — cydonia
/// decodes nothing the default would turn away.
pub fn init(cx: &mut App) {
    editor::set_image_store(
        cx,
        ImageStore {
            keep,
            ..ImageStore::default()
        },
    );
}

/// Point at the article a document was opened from — its `content.md`, which
/// is the file the pictures sit beside.
pub fn aim(content: Option<&Path>) {
    if let Ok(mut target) = TARGET.lock() {
        *target = content.and_then(Path::parent).map(Path::to_path_buf);
    }
}

/// Take a picture into the open article's directory, and answer with what the
/// document is to point at.
///
/// Named for the bytes' own hash, so the same screenshot pasted twice is the
/// one file — and gpui, which caches a decoded picture against its path, is
/// handed the copy it already has.
///
/// An absolute path rather than a relative one: what paints the picture reads
/// the URL as a path off this process, whose working directory is not the
/// article's.
fn keep(source: Source, _editor: &Entity<Editor>, _cx: &App) -> Option<String> {
    let dir = TARGET.lock().ok()?.clone()?;
    let (bytes, extension) = match source {
        Source::Bytes(image) => (
            Cow::Borrowed(image.bytes.as_slice()),
            image.format.extension().to_owned(),
        ),
        // A dropped file is copied in rather than pointed at where it is: a
        // document that outlives the download it was written from is the whole
        // reason the picture lives with the article.
        Source::File(path) => (
            Cow::Owned(std::fs::read(path).ok()?),
            path.extension()?.to_str()?.to_ascii_lowercase(),
        ),
    };
    let file = dir.join(format!("{MARK}{:x}.{extension}", hash(&bytes)));
    if !file.is_file() && std::fs::write(&file, &bytes).is_err() {
        return None;
    }
    Some(file.to_string_lossy().into_owned())
}
