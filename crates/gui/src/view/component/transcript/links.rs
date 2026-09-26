use bezel::gpui;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = cydonia, no_json)]
pub struct OpenSessionFile {
    pub session: u64,
    pub path: PathBuf,
    pub line: Option<usize>,
}

pub(super) fn resolve(cwd: &Path, href: &str) -> Option<(PathBuf, Option<usize>)> {
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
    let url = match url::Url::parse(target) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            crate::model::file_url::from_dir(cwd)?.join(target).ok()?
        }
        Err(_) => return None,
    };
    if url.scheme() != "file" {
        return None;
    }
    Some((crate::model::file_url::to_path(&url)?, line))
}

#[cfg(test)]
#[path = "../../../../tests/unit/session_links.rs"]
mod tests;
