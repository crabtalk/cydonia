//! Where a project is.
//!
//! Written to a file as one string: the bare path a [`Location::Local`]
//! holds, the shape `state.toml` and spaces held before this type, and
//! `cupola://<host id><root>` for a [`Location::Remote`]. Read back, a string
//! that is not a well-formed `cupola://` location is a local path.

#[cfg(feature = "remote")]
use cupola::client::host::HostId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    path::{Path, PathBuf},
};

/// What a remote location's stored form begins with.
#[cfg(feature = "remote")]
const SCHEME: &str = "cupola://";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Location {
    /// A directory on this machine.
    Local(PathBuf),
    /// A directory on a host Cupola's controller saved. `root` is absolute.
    #[cfg(feature = "remote")]
    Remote { host: HostId, root: PathBuf },
}

impl Location {
    /// The project's directory, on whichever machine holds it.
    pub fn root(&self) -> &Path {
        match self {
            Self::Local(root) => root,
            #[cfg(feature = "remote")]
            Self::Remote { root, .. } => root,
        }
    }

    /// The project's directory, where that is on this machine.
    pub fn local(&self) -> Option<&Path> {
        match self {
            Self::Local(root) => Some(root),
            #[cfg(feature = "remote")]
            Self::Remote { .. } => None,
        }
    }

    /// The host the project is on, where that is not this machine.
    #[cfg(feature = "remote")]
    pub fn host(&self) -> Option<HostId> {
        match self {
            Self::Local(_) => None,
            Self::Remote { host, .. } => Some(*host),
        }
    }

    /// `root`, on the same machine as this location.
    pub fn with_root(&self, root: PathBuf) -> Self {
        match self {
            Self::Local(_) => Self::Local(root),
            #[cfg(feature = "remote")]
            Self::Remote { host, .. } => Self::Remote { host: *host, root },
        }
    }

    /// `path`, a path on this location's machine, canonicalized where that
    /// machine is this one and the path resolves; as given otherwise.
    pub fn settle(&self, path: &Path) -> PathBuf {
        match self {
            Self::Local(_) => path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
            #[cfg(feature = "remote")]
            Self::Remote { .. } => path.to_path_buf(),
        }
    }

    /// This location with its root [settled](Self::settle).
    pub fn settled(&self) -> Self {
        self.with_root(self.settle(self.root()))
    }

    /// What a stored string names.
    pub fn parse(stored: &str) -> Self {
        Self::read(PathBuf::from(stored))
    }

    fn read(path: PathBuf) -> Self {
        #[cfg(feature = "remote")]
        if let Some(remote) = Self::remote(&path) {
            return remote;
        }
        Self::Local(path)
    }

    /// `cupola://<host id><root>`, `root` absolute.
    #[cfg(feature = "remote")]
    fn remote(path: &Path) -> Option<Self> {
        let rest = path.to_str()?.strip_prefix(SCHEME)?;
        let slash = rest.find('/')?;
        let host = rest[..slash].parse().ok()?;
        Some(Self::Remote {
            host,
            root: PathBuf::from(&rest[slash..]),
        })
    }
}

impl From<PathBuf> for Location {
    fn from(root: PathBuf) -> Self {
        Self::Local(root)
    }
}

impl From<&Path> for Location {
    fn from(root: &Path) -> Self {
        Self::Local(root.to_path_buf())
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(root) => root.display().fmt(f),
            #[cfg(feature = "remote")]
            Self::Remote { host, root } => write!(f, "{SCHEME}{host}{}", root.display()),
        }
    }
}

/// Refused for a root that is not UTF-8, as a bare path is.
impl Serialize for Location {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Local(root) => root.serialize(serializer),
            #[cfg(feature = "remote")]
            Self::Remote { host, root } => {
                let root = root
                    .to_str()
                    .ok_or_else(|| serde::ser::Error::custom("path contains invalid UTF-8"))?;
                serializer.serialize_str(&format!("{SCHEME}{host}{root}"))
            }
        }
    }
}

impl<'de> Deserialize<'de> for Location {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PathBuf::deserialize(deserializer).map(Self::read)
    }
}
