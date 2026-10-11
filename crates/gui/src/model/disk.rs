//! The project files the right panel lists and edits. The filesystem with the
//! `desktop` feature; without it, a tree held in memory for as long as the
//! process runs, filled by [`Memory::seed`].

use std::{
    io,
    path::{Path, PathBuf},
};

/// Where this build's project files are.
#[cfg(feature = "desktop")]
pub type Here = Disk;
#[cfg(not(feature = "desktop"))]
pub type Here = Memory;

/// Project files, read and written.
pub trait Files {
    /// What is directly under `path`, each as whether it is a directory and
    /// where it is.
    fn read_dir(path: &Path) -> io::Result<Vec<(bool, PathBuf)>>;

    /// The file's bytes: at most `limit + 1` of them, so a read longer than
    /// `limit` says the file is.
    fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>>;

    /// Put `bytes` where `path` is. `check` runs before the file is replaced;
    /// an error from it, or a failed write, leaves the file as it was.
    fn replace(path: &Path, bytes: &[u8], check: impl FnOnce() -> io::Result<()>)
    -> io::Result<()>;
}

#[cfg(feature = "desktop")]
pub struct Disk;

#[cfg(feature = "desktop")]
impl Files for Disk {
    fn read_dir(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
        std::fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                Ok((entry.file_type()?.is_dir(), entry.path()))
            })
            .collect()
    }

    fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        use std::io::Read as _;
        if !std::fs::metadata(path)?.is_file() {
            return Err(io::Error::other("Choose a regular file"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn replace(
        path: &Path,
        bytes: &[u8],
        check: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        artifact::fs::save_checked(path, bytes, check)
    }
}

#[cfg(not(feature = "desktop"))]
pub struct Memory;

#[cfg(not(feature = "desktop"))]
impl Memory {
    /// Put `files`, named relative to `root` with `/` between components,
    /// under `root`.
    pub fn seed<'a>(root: &Path, files: impl IntoIterator<Item = (&'a str, &'a [u8])>) {
        let mut held = Self::held();
        for (name, bytes) in files {
            held.insert(root.join(name), bytes.to_vec());
        }
    }

    fn held() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<PathBuf, Vec<u8>>> {
        static HELD: std::sync::Mutex<std::collections::BTreeMap<PathBuf, Vec<u8>>> =
            std::sync::Mutex::new(std::collections::BTreeMap::new());
        HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(not(feature = "desktop"))]
impl Files for Memory {
    fn read_dir(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
        let held = Self::held();
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

    fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
        let _ = limit;
        Self::held()
            .get(path)
            .cloned()
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn replace(
        path: &Path,
        bytes: &[u8],
        check: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        check()?;
        Self::held().insert(path.to_owned(), bytes.to_vec());
        Ok(())
    }
}
