//! Paths as the bytes the platform names them by. Off Unix, a name that is not
//! UTF-8 is replaced lossily.

use std::{borrow::Cow, ffi::OsString, path::Path, path::PathBuf};

/// The path a name of bytes from git or the wire stands for.
pub(crate) fn path(bytes: Vec<u8>) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        OsString::from_vec(bytes).into()
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(&bytes).into_owned()).into()
    }
}

/// The bytes `path` is named by.
pub(crate) fn bytes(path: &Path) -> Cow<'_, [u8]> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Cow::Borrowed(path.as_os_str().as_bytes())
    }
    #[cfg(not(unix))]
    {
        match path.to_string_lossy() {
            Cow::Borrowed(text) => Cow::Borrowed(text.as_bytes()),
            Cow::Owned(text) => Cow::Owned(text.into_bytes()),
        }
    }
}

pub(crate) mod path {
    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};
    use std::path::{Path, PathBuf};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        super::bytes(path).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        Vec::<u8>::deserialize(deserializer).map(super::path)
    }
}

pub(crate) mod optional_path {
    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S: Serializer>(
        path: &Option<PathBuf>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        path.as_deref().map(super::bytes).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PathBuf>, D::Error> {
        Option::<Vec<u8>>::deserialize(deserializer).map(|bytes| bytes.map(super::path))
    }
}
