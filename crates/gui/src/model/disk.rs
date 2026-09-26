//! The project files the right panel lists and edits. The filesystem with the
//! `desktop` feature; without it, a tree held in memory for as long as the
//! process runs, filled by [`seed`].

use std::{
    io,
    path::{Path, PathBuf},
};

/// What is directly under `path`, each as whether it is a directory and where
/// it is.
#[cfg(feature = "desktop")]
pub fn read_dir(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
    std::fs::read_dir(path)?
        .map(|entry| {
            let entry = entry?;
            Ok((entry.file_type()?.is_dir(), entry.path()))
        })
        .collect()
}

#[cfg(not(feature = "desktop"))]
pub fn read_dir(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
    let held = held();
    let mut found: Vec<(bool, PathBuf)> = Vec::new();
    for file in held.keys() {
        let Ok(rest) = file.strip_prefix(path) else {
            continue;
        };
        let mut parts = rest.components();
        let Some(first) = parts.next() else {
            continue;
        };
        let child = path.join(first);
        let directory = parts.next().is_some();
        if !found.iter().any(|(_, seen)| *seen == child) {
            found.push((directory, child));
        }
    }
    match found.is_empty() {
        true => Err(io::ErrorKind::NotFound.into()),
        false => Ok(found),
    }
}

#[cfg(not(feature = "desktop"))]
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    held()
        .get(path)
        .cloned()
        .ok_or_else(|| io::ErrorKind::NotFound.into())
}

#[cfg(not(feature = "desktop"))]
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    held().insert(path.to_owned(), bytes.to_vec());
    Ok(())
}

/// Put `files`, named relative to `root` with `/` between components, under
/// `root`.
#[cfg(not(feature = "desktop"))]
pub fn seed<'a>(root: &Path, files: impl IntoIterator<Item = (&'a str, &'a [u8])>) {
    let mut held = held();
    for (name, bytes) in files {
        held.insert(root.join(name), bytes.to_vec());
    }
}

#[cfg(not(feature = "desktop"))]
fn held() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<PathBuf, Vec<u8>>> {
    static HELD: std::sync::Mutex<std::collections::BTreeMap<PathBuf, Vec<u8>>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
