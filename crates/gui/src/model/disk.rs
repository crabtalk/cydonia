//! The project files the right panel lists and edits, through the
//! [`Backend`] [`Backend::open`] picks for the project. This build's own is
//! [`Here`]: the filesystem with the `desktop` feature; without it, a tree
//! held in memory for as long as the process runs, filled by
//! [`Memory::seed`]. With the `desktop` feature, [`Remote`] is the files on
//! another machine.
//!
//! Every call answers with a future. [`Here`]'s does its work when first
//! polled and is then ready.

use artifact::location::Location;
use std::{
    future::Future,
    io,
    path::{Path, PathBuf},
};

/// What a refused [`Files::replace`] says.
pub const CHANGED: &str = "File changed on disk. Reload or overwrite to continue.";

/// Where this build's project files are.
#[cfg(feature = "desktop")]
pub type Here = Disk;
#[cfg(not(feature = "desktop"))]
pub type Here = Memory;

/// Project files, read and written.
pub trait Files {
    /// What is directly under `path`, each as whether it is a directory and
    /// where it is. A symlink is not a directory.
    fn read_dir(
        &self,
        path: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<Self>;

    /// The file's first `limit` bytes.
    fn read(
        &self,
        path: &Path,
        limit: u64,
    ) -> impl Future<Output = io::Result<Contents>> + Send + use<Self>;

    /// Put `bytes` where `path` is. With `unchanged`, refused with [`CHANGED`]
    /// unless the file holds exactly those bytes; a refused or failed write
    /// leaves the file as it was.
    fn replace(
        &self,
        path: &Path,
        bytes: Vec<u8>,
        unchanged: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<Self>;

    /// Whether its paths are on this machine, for other applications to open.
    fn local(&self) -> bool;
}

/// What [`Files::read`] answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contents {
    pub bytes: Vec<u8>,
    /// Whether the file holds more than `bytes`.
    pub cut: bool,
}

/// The files of one project, wherever they are.
#[derive(Clone)]
pub enum Backend {
    Here(Here),
    #[cfg(feature = "desktop")]
    Remote(Remote),
    #[cfg(feature = "desktop")]
    Unreachable(Unreachable),
}

impl Contents {
    /// The first `limit` of `bytes`.
    fn cut(mut bytes: Vec<u8>, limit: u64) -> Self {
        let cut = bytes.len() as u64 > limit;
        bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Self { bytes, cut }
    }
}

impl Backend {
    /// The files of the project at `location`: [`Unreachable`] for a location
    /// on a host whose client cannot be had.
    pub fn open(location: &Location) -> Self {
        match location {
            Location::Local(_) => Self::Here(Here::default()),
            #[cfg(feature = "desktop")]
            Location::Remote { host, root } => {
                match crate::model::remote::Remotes::get().and_then(|r| r.client(*host)) {
                    Ok(client) => Self::Remote(Remote::new(&client, root.clone())),
                    Err(error) => Self::Unreachable(Unreachable(format!("{error:#}").into())),
                }
            }
        }
    }
}

/// The same call on whichever backend is held, its future boxed.
macro_rules! each {
    ($self:ident, $files:ident => $call:expr) => {{
        use futures::FutureExt as _;
        match $self {
            Backend::Here($files) => $call.boxed(),
            #[cfg(feature = "desktop")]
            Backend::Remote($files) => $call.boxed(),
            #[cfg(feature = "desktop")]
            Backend::Unreachable($files) => $call.boxed(),
        }
    }};
}

impl Files for Backend {
    fn read_dir(
        &self,
        path: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<> {
        each!(self, files => files.read_dir(path))
    }

    fn read(
        &self,
        path: &Path,
        limit: u64,
    ) -> impl Future<Output = io::Result<Contents>> + Send + use<> {
        each!(self, files => files.read(path, limit))
    }

    fn replace(
        &self,
        path: &Path,
        bytes: Vec<u8>,
        unchanged: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<> {
        each!(self, files => files.replace(path, bytes, unchanged))
    }

    fn local(&self) -> bool {
        match self {
            Backend::Here(files) => files.local(),
            #[cfg(feature = "desktop")]
            Backend::Remote(files) => files.local(),
            #[cfg(feature = "desktop")]
            Backend::Unreachable(files) => files.local(),
        }
    }
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Default)]
pub struct Disk;

#[cfg(feature = "desktop")]
impl Disk {
    fn list(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
        std::fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                Ok((entry.file_type()?.is_dir(), entry.path()))
            })
            .collect()
    }

    fn take(path: &Path, limit: u64) -> io::Result<Contents> {
        use std::io::Read as _;
        if !std::fs::metadata(path)?.is_file() {
            return Err(io::Error::other("Choose a regular file"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(Contents::cut(bytes, limit))
    }
}

#[cfg(feature = "desktop")]
impl Files for Disk {
    fn read_dir(
        &self,
        path: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<> {
        let path = path.to_owned();
        async move { Self::list(&path) }
    }

    fn read(
        &self,
        path: &Path,
        limit: u64,
    ) -> impl Future<Output = io::Result<Contents>> + Send + use<> {
        let path = path.to_owned();
        async move { Self::take(&path, limit) }
    }

    fn replace(
        &self,
        path: &Path,
        bytes: Vec<u8>,
        unchanged: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<> {
        let path = path.to_owned();
        async move {
            artifact::fs::save_checked(&path, bytes, || match unchanged {
                Some(unchanged) if std::fs::read(&path)? != unchanged => {
                    Err(io::Error::other(CHANGED))
                }
                _ => Ok(()),
            })
        }
    }

    fn local(&self) -> bool {
        true
    }
}

#[cfg(not(feature = "desktop"))]
#[derive(Clone, Copy, Default)]
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

    fn list(path: &Path) -> io::Result<Vec<(bool, PathBuf)>> {
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
}

#[cfg(not(feature = "desktop"))]
impl Files for Memory {
    fn read_dir(
        &self,
        path: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<> {
        let path = path.to_owned();
        async move { Self::list(&path) }
    }

    fn read(
        &self,
        path: &Path,
        limit: u64,
    ) -> impl Future<Output = io::Result<Contents>> + Send + use<> {
        let path = path.to_owned();
        async move {
            let bytes = Self::held()
                .get(&path)
                .cloned()
                .ok_or(io::ErrorKind::NotFound)?;
            Ok(Contents::cut(bytes, limit))
        }
    }

    fn replace(
        &self,
        path: &Path,
        bytes: Vec<u8>,
        unchanged: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<> {
        let path = path.to_owned();
        async move {
            let mut held = Self::held();
            if unchanged.is_some_and(|unchanged| held.get(&path) != Some(&unchanged)) {
                return Err(io::Error::other(CHANGED));
            }
            held.insert(path, bytes);
            Ok(())
        }
    }

    fn local(&self) -> bool {
        true
    }
}

/// The files of a project whose host's client could not be had: every call
/// is refused with why.
#[cfg(feature = "desktop")]
#[derive(Clone)]
pub struct Unreachable(std::sync::Arc<str>);

#[cfg(feature = "desktop")]
impl Unreachable {
    fn refused<T>(&self) -> std::future::Ready<io::Result<T>> {
        std::future::ready(Err(io::Error::other(self.0.to_string())))
    }
}

#[cfg(feature = "desktop")]
impl Files for Unreachable {
    fn read_dir(
        &self,
        _: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<> {
        self.refused()
    }

    fn read(&self, _: &Path, _: u64) -> impl Future<Output = io::Result<Contents>> + Send + use<> {
        self.refused()
    }

    fn replace(
        &self,
        _: &Path,
        _: Vec<u8>,
        _: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<> {
        self.refused()
    }

    fn local(&self) -> bool {
        false
    }
}

/// The files on the machine a Cupola server runs on, through its `fs`
/// service. Paths are on that machine; a relative one is under `root`.
#[cfg(feature = "desktop")]
#[derive(Clone)]
pub struct Remote {
    fs: cupola::Handle<cupola::fs::Fs>,
    root: PathBuf,
}

#[cfg(feature = "desktop")]
impl Remote {
    /// The project at `root` on the server `client` reaches.
    pub fn new(client: &cupola::Client, root: PathBuf) -> Self {
        Self {
            fs: client.service(),
            root,
        }
    }

    /// The project's directory on the server's machine.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn unexpected(response: cupola::fs::Response) -> io::Error {
        io::Error::other(format!("fs answered {response:?}"))
    }

    fn failed(error: cupola::Error) -> io::Error {
        io::Error::other(error.to_string())
    }
}

#[cfg(feature = "desktop")]
impl Files for Remote {
    fn read_dir(
        &self,
        path: &Path,
    ) -> impl Future<Output = io::Result<Vec<(bool, PathBuf)>>> + Send + use<> {
        use cupola::fs::{Kind, Request, Response};
        let path = self.root.join(path);
        let listed = self.fs.send(&Request::List { path: path.clone() });
        async move {
            match listed.await.map_err(Self::failed)? {
                Response::List(entries) => Ok(entries
                    .into_iter()
                    .map(|entry| (entry.kind == Kind::Dir, path.join(entry.name)))
                    .collect()),
                other => Err(Self::unexpected(other)),
            }
        }
    }

    fn read(
        &self,
        path: &Path,
        limit: u64,
    ) -> impl Future<Output = io::Result<Contents>> + Send + use<> {
        use cupola::fs::{Request, Response};
        let read = self.fs.send(&Request::Read {
            path: self.root.join(path),
            limit: Some(limit),
        });
        async move {
            match read.await.map_err(Self::failed)? {
                Response::Read { bytes, cut } => Ok(Contents { bytes, cut }),
                other => Err(Self::unexpected(other)),
            }
        }
    }

    fn replace(
        &self,
        path: &Path,
        bytes: Vec<u8>,
        unchanged: Option<Vec<u8>>,
    ) -> impl Future<Output = io::Result<()>> + Send + use<> {
        use cupola::fs::{Expect, Hash, Request, Response};
        let written = self.fs.send(&Request::Write {
            path: self.root.join(path),
            bytes,
            expect: unchanged.map_or(Expect::Anything, |held| Expect::Holds(Hash::of(&held))),
        });
        async move {
            match written.await.map_err(Self::failed)? {
                Response::Written => Ok(()),
                Response::Refused => Err(io::Error::other(CHANGED)),
                other => Err(Self::unexpected(other)),
            }
        }
    }

    fn local(&self) -> bool {
        false
    }
}
