//! Writing a file whole.

use std::{
    io::{self, Write as _},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

/// The temporary files [`save`] has made in this process.
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Write `contents` to `path` through a temporary file beside it, renamed over
/// it: a reader sees the old contents or the new, never part of either. The
/// directory is made where it is missing.
pub fn save(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("a file to save has a directory"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = dir.join(format!(
        ".{name}.{}-{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}
