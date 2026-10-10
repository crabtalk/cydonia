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

/// The file a link names on this machine, and the line it points at:
/// `src/main.rs:42`, `src/main.rs:42:7` and `src/main.rs#L42` all name line
/// 42. A relative path is joined onto `base`, and names nothing without one.
pub fn target(base: Option<&Path>, href: &str) -> Option<(PathBuf, Option<usize>)> {
    if href.is_empty() || href.starts_with('#') || href.starts_with("//") {
        return None;
    }
    let mut target = href;
    let mut line = None;
    if let Some((path, fragment)) = target.rsplit_once('#') {
        let number = fragment.strip_prefix('L').unwrap_or(fragment);
        let end = number
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(number.len());
        line = number[..end].parse::<usize>().ok().filter(|n| *n > 0);
        if line.is_some() {
            target = path;
        }
    }
    if let Some((path, last)) = target.rsplit_once(':')
        && let Ok(number) = last.parse::<usize>()
    {
        target = path;
        line = Some(number.max(1));
        if let Some((path, first)) = target.rsplit_once(':')
            && let Ok(number) = first.parse::<usize>()
        {
            target = path;
            line = Some(number.max(1));
        }
    }
    if let Some(rest) = target.strip_prefix("~/") {
        return Some((dirs::home_dir()?.join(rest), line));
    }
    let url = match Url::parse(target) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let base = match base {
                Some(base) => base,
                None if target.starts_with('/') => Path::new("/"),
                None => return None,
            };
            from_dir(base)?.join(target).ok()?
        }
        Err(_) => return None,
    };
    if url.scheme() != "file" {
        return None;
    }
    Some((to_path(&url)?, line))
}

#[cfg(test)]
#[path = "../../tests/unit/file_links.rs"]
mod tests;
