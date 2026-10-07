//! A document directory: `content.md`, a `properties.toml` beside it, and the
//! pictures its body holds under `assets/`. An article is one, and so is a
//! card.
//!
//! The properties file is edited in place with `toml_edit`, so keys cydonia
//! does not know about — an agent's, a later version's — survive a write.

use crate::{project::fs, stamp};
use std::path::{Path, PathBuf};

pub const CONTENT: &str = "content.md";
pub const PROPERTIES: &str = "properties.toml";
pub const ASSETS: &str = "assets";

/// The document inside a document directory.
pub fn content(dir: &Path) -> PathBuf {
    dir.join(CONTENT)
}

/// Where the pictures in a document's body go, whether or not any have been
/// written. Takes the path of its `content.md`.
pub fn assets(content: &Path) -> PathBuf {
    content.with_file_name(ASSETS)
}

/// The properties file beside a `content.md`.
pub fn properties(content: &Path) -> PathBuf {
    content.with_file_name(PROPERTIES)
}

/// When the document was last written, whichever of its two files took the
/// write. The properties file is only asked about when it is there:
/// [`stamp::of`] answers `now` for a file it cannot stat.
pub fn touched(content: &Path) -> u128 {
    let written = stamp::of(content);
    match properties(content) {
        path if path.is_file() => written.max(stamp::of(&path)),
        _ => written,
    }
}

// ── properties ───────────────────────────────────────────────────

/// A properties file's text, parsed. Empty for a file that is missing or does
/// not parse.
pub fn parse(text: &str) -> toml_edit::DocumentMut {
    text.parse().unwrap_or_default()
}

/// `text` with each field written in, or taken out where it is `None`, keeping
/// every other key. `None` when nothing is left, which is a file that should
/// not exist.
pub fn apply(text: &str, fields: &[(&str, Option<toml_edit::Item>)]) -> Option<String> {
    let mut doc = parse(text);
    for (key, value) in fields {
        match value {
            Some(value) => doc[*key] = value.clone(),
            None => {
                doc.remove(key);
            }
        }
    }
    (!doc.is_empty()).then(|| doc.to_string())
}

/// Write fields into the properties file beside `content`, removing the file
/// when nothing is left in it.
pub fn save(content: &Path, fields: &[(&str, Option<toml_edit::Item>)]) -> std::io::Result<()> {
    let path = properties(content);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match apply(&text, fields) {
        Some(text) => std::fs::write(&path, text),
        None => match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

// ── moving one ───────────────────────────────────────────────────

/// Move a directory, falling back to a copy where a rename cannot cross what
/// is between the two — a project on another disk is the usual reason.
pub fn carry(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from)
}

pub fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        match entry.file_type()?.is_dir() {
            true => copy_dir(&source, &target)?,
            false => {
                std::fs::copy(&source, &target)?;
            }
        }
    }
    Ok(())
}

/// Point the body at where its own directory is now: whole paths into the
/// directory it came from are rewritten to the one it is in. Best effort.
pub fn repoint(content: &Path, from: &Path, to: &Path) {
    let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
    let Ok(text) = std::fs::read_to_string(content) else {
        return;
    };
    if !text.contains(from.as_ref()) {
        return;
    }
    let _ = std::fs::write(content, text.replace(from.as_ref(), to.as_ref()));
}

/// Bring the pictures the body points at in one project's shared `assets/`
/// to another project's, and point the body there. The file names are hashes
/// of the bytes, so a picture already in the destination is the same file.
/// Best effort.
pub fn carry_assets(content: &Path, from: &Path, to: &Path) {
    let Ok(text) = std::fs::read_to_string(content) else {
        return;
    };
    let (here, there) = (
        fs::Project::new(from).assets(),
        fs::Project::new(to).assets(),
    );
    let (here, there) = (here.to_string_lossy(), there.to_string_lossy());
    if !text.contains(here.as_ref()) {
        return;
    }
    for name in assets_named(&text, &here) {
        let (source, target) = (
            Path::new(here.as_ref()).join(&name),
            Path::new(there.as_ref()).join(&name),
        );
        if target.exists() {
            continue;
        }
        if std::fs::create_dir_all(there.as_ref()).is_ok() {
            let _ = std::fs::copy(&source, &target);
        }
    }
    let _ = std::fs::write(content, text.replace(here.as_ref(), there.as_ref()));
}

/// `text` with the pictures it points at in `shared` copied into `own`, and
/// pointed at there as `assets/<name>`. A picture that cannot be copied keeps
/// its whole path. The shared copies stay.
pub fn adopt_assets(text: &str, shared: &Path, own: &Path) -> String {
    let shared = shared.to_string_lossy();
    if !text.contains(shared.as_ref()) {
        return text.to_owned();
    }
    let mut text = text.to_owned();
    for name in assets_named(&text, &shared) {
        let source = Path::new(shared.as_ref()).join(&name);
        if std::fs::create_dir_all(own).is_ok() && std::fs::copy(&source, own.join(&name)).is_ok() {
            text = text.replace(
                &source.to_string_lossy().into_owned(),
                &format!("{ASSETS}/{name}"),
            );
        }
    }
    text
}

/// The file names under `assets` the document mentions. Everything up to what
/// cannot be in one: a path in markdown is followed by a quote, a bracket or
/// the end of the line, and none of those are in a name this app writes.
fn assets_named(text: &str, assets: &str) -> Vec<String> {
    let mut names = Vec::new();
    for rest in text.split(assets).skip(1) {
        let rest = rest.strip_prefix(std::path::MAIN_SEPARATOR).unwrap_or(rest);
        let name: String = rest
            .chars()
            .take_while(|c| !matches!(c, '"' | '\'' | ')' | ']' | '>' | '\n' | '\r' | ' '))
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}
