//! `file://` URLs and the paths they name. wasm32 has neither: every answer
//! there is `None`.

use std::path::{Path, PathBuf};
use url::Url;

pub fn to_path(url: &Url) -> Option<PathBuf> {
    #[cfg(not(target_family = "wasm"))]
    return url.to_file_path().ok();
    #[cfg(target_family = "wasm")]
    {
        let _ = url;
        None
    }
}

pub fn from_path(path: &Path) -> Option<Url> {
    #[cfg(not(target_family = "wasm"))]
    return Url::from_file_path(path).ok();
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        None
    }
}

/// A directory as a base to join relative paths onto.
pub fn from_dir(path: &Path) -> Option<Url> {
    #[cfg(not(target_family = "wasm"))]
    return Url::from_directory_path(path).ok();
    #[cfg(target_family = "wasm")]
    {
        let _ = path;
        None
    }
}
