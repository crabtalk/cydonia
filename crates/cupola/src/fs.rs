//! The `fs` service: the server's files, listed, read and written.

use std::{
    ffi::OsString,
    fs::{self, File, Permissions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tempfile::NamedTempFile;

use crate::{Server, Service};

/// The files on the server's machine. Relative paths are relative to the
/// server process's working directory.
pub struct Fs {
    /// Held from a write's expectation check to its rename, so no other write
    /// through this server lands between them.
    writes: Mutex<()>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Answered with [`Response::List`].
    List {
        #[serde(with = "crate::bytes::path")]
        path: PathBuf,
    },
    /// Answered with [`Response::Read`], holding at most `limit` bytes.
    Read {
        #[serde(with = "crate::bytes::path")]
        path: PathBuf,
        limit: Option<u64>,
    },
    /// Answered with [`Response::Written`] once `bytes` replace the file
    /// whole, or [`Response::Refused`], with nothing written, when the file
    /// does not hold what `expect` says.
    ///
    /// The bytes go to a temporary file in the same directory, synced, then
    /// renamed over `path`; a symlink at `path` is followed. The file keeps its
    /// permissions; its owner, extended attributes and hard links are not
    /// kept. A new file gets the permissions `fs::write` would give it.
    Write {
        #[serde(with = "crate::bytes::path")]
        path: PathBuf,
        bytes: Vec<u8>,
        expect: Expect,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// Sorted by name.
    List(Vec<Entry>),
    /// `cut` is whether the file holds more than `bytes`.
    Read {
        bytes: Vec<u8>,
        cut: bool,
    },
    Written,
    /// The file did not hold what the write expected; it was left as it was.
    Refused,
}

/// What a write expects the file to hold when it lands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expect {
    /// Whatever it holds, or that it does not exist.
    Anything,
    /// That nothing exists at the path.
    Absent,
    /// A file whose content hashes to this.
    Holds(Hash),
}

/// The SHA-256 of a file's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

/// One name in a listed directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(with = "crate::bytes::os_string")]
    pub name: OsString,
    pub kind: Kind,
}

/// What an entry is, without following a symlink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

/// Bytes written and synced to a temporary file beside the file they are to
/// replace. Dropped without [`Replacement::land`], the temporary file is
/// removed.
pub(crate) struct Replacement {
    temp: NamedTempFile,
}

/// The permissions this platform gives a file it creates.
trait Fresh {
    /// What a temporary file is created with in place of `tempfile`'s
    /// owner-only default, before the umask; `None` keeps the default.
    fn permissions() -> Option<Permissions>;
}

/// Read and write for everyone, less the umask.
#[cfg(unix)]
struct Unix;

/// `tempfile`'s default.
#[cfg(not(unix))]
struct Other;

#[cfg(unix)]
type Platform = Unix;
#[cfg(not(unix))]
type Platform = Other;

impl Service for Fs {
    const NAME: &'static str = "fs";
    type Request = Request;
    type Response = Response;
    type Event = ();
}

impl Fs {
    /// Answers [`Fs`] requests on `server`, each on a blocking thread.
    pub fn register(server: &Server) {
        let fs = Arc::new(Self {
            writes: Mutex::new(()),
        });
        server.register::<Self, _, _>(move |request| {
            let fs = fs.clone();
            blocking::unblock(move || fs.answer(request))
        });
    }

    fn answer(&self, request: Request) -> anyhow::Result<Response> {
        match request {
            Request::List { path } => {
                let mut entries = fs::read_dir(&path)
                    .with_context(|| format!("list {}", path.display()))?
                    .map(|entry| {
                        let entry = entry?;
                        Ok(Entry {
                            name: entry.file_name(),
                            kind: entry.file_type()?.into(),
                        })
                    })
                    .collect::<io::Result<Vec<_>>>()
                    .with_context(|| format!("list {}", path.display()))?;
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(Response::List(entries))
            }
            Request::Read { path, limit } => {
                Self::read(&path, limit).with_context(|| format!("read {}", path.display()))
            }
            Request::Write {
                path,
                bytes,
                expect,
            } => self
                .write(&path, &bytes, &expect)
                .with_context(|| format!("write {}", path.display())),
        }
    }

    fn read(path: &Path, limit: Option<u64>) -> io::Result<Response> {
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        let Some(limit) = limit else {
            file.read_to_end(&mut bytes)?;
            return Ok(Response::Read { bytes, cut: false });
        };
        file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
        let cut = bytes.len() as u64 > limit;
        bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        Ok(Response::Read { bytes, cut })
    }

    fn write(&self, path: &Path, bytes: &[u8], expect: &Expect) -> io::Result<Response> {
        let target = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
            _ => path.to_owned(),
        };
        let replacement = Replacement::beside(&target, bytes)?;
        let _writes = self
            .writes
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !expect.holds(&target)? {
            return Ok(Response::Refused);
        }
        replacement.land(&target)?;
        Ok(Response::Written)
    }
}

impl Expect {
    /// Whether what is at `path` now is what this expects.
    fn holds(&self, path: &Path) -> io::Result<bool> {
        match self {
            Self::Anything => Ok(true),
            Self::Absent => match fs::symlink_metadata(path) {
                Ok(_) => Ok(false),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
                Err(error) => Err(error),
            },
            Self::Holds(hash) => match Hash::of_file(path) {
                Ok(found) => Ok(found == *hash),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error),
            },
        }
    }
}

impl Hash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    fn of_file(path: &Path) -> io::Result<Self> {
        let mut hasher = Sha256::new();
        io::copy(&mut File::open(path)?, &mut hasher)?;
        Ok(Self(hasher.finalize().into()))
    }
}

impl Replacement {
    /// Writes and syncs `bytes` to a new temporary file in `path`'s directory.
    pub(crate) fn beside(path: &Path, bytes: &[u8]) -> io::Result<Self> {
        let dir = match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => dir,
            _ => Path::new("."),
        };
        let mut builder = tempfile::Builder::new();
        if let Some(permissions) = Platform::permissions() {
            builder.permissions(permissions);
        }
        let mut temp = builder.tempfile_in(dir)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        Ok(Self { temp })
    }

    /// Renames the temporary file over `path`, giving it the permissions of
    /// the file it replaces.
    pub(crate) fn land(self, path: &Path) -> io::Result<()> {
        match fs::metadata(path) {
            Ok(metadata) => self
                .temp
                .as_file()
                .set_permissions(metadata.permissions())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.temp.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

#[cfg(unix)]
impl Fresh for Unix {
    fn permissions() -> Option<Permissions> {
        use std::os::unix::fs::PermissionsExt as _;
        Some(Permissions::from_mode(0o666))
    }
}

#[cfg(not(unix))]
impl Fresh for Other {
    fn permissions() -> Option<Permissions> {
        None
    }
}

impl From<fs::FileType> for Kind {
    fn from(file_type: fs::FileType) -> Self {
        if file_type.is_symlink() {
            Self::Symlink
        } else if file_type.is_dir() {
            Self::Dir
        } else if file_type.is_file() {
            Self::File
        } else {
            Self::Other
        }
    }
}
