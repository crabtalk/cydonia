//! Paths and OS strings as the bytes the platform names them by, for payloads
//! that must carry names that are not UTF-8. The submodules are serde `with`
//! modules for fields of those types.
//!
//! Off Unix, a name that is not UTF-8 is replaced lossily.

use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use serde::{Serialize, Serializer};

/// How this platform turns names into bytes and back.
pub trait Names {
    /// The name `bytes` stands for.
    fn os_string(bytes: Vec<u8>) -> OsString;

    /// The bytes `name` is named by.
    fn bytes(name: &OsStr) -> Cow<'_, [u8]>;

    /// The path `bytes` stands for.
    fn path(bytes: Vec<u8>) -> PathBuf {
        Self::os_string(bytes).into()
    }
}

/// Names are bytes as they are.
#[cfg(unix)]
pub struct Unix;

/// Names are UTF-8.
#[cfg(not(unix))]
pub struct Lossy;

#[cfg(unix)]
pub type Platform = Unix;
#[cfg(not(unix))]
pub type Platform = Lossy;

/// A name that serializes as its bytes.
struct Name<'a>(&'a OsStr);

#[cfg(unix)]
impl Names for Unix {
    fn os_string(bytes: Vec<u8>) -> OsString {
        use std::os::unix::ffi::OsStringExt as _;
        OsString::from_vec(bytes)
    }

    fn bytes(name: &OsStr) -> Cow<'_, [u8]> {
        use std::os::unix::ffi::OsStrExt as _;
        Cow::Borrowed(name.as_bytes())
    }
}

#[cfg(not(unix))]
impl Names for Lossy {
    fn os_string(bytes: Vec<u8>) -> OsString {
        OsString::from(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn bytes(name: &OsStr) -> Cow<'_, [u8]> {
        match name.to_string_lossy() {
            Cow::Borrowed(text) => Cow::Borrowed(text.as_bytes()),
            Cow::Owned(text) => Cow::Owned(text.into_bytes()),
        }
    }
}

impl Serialize for Name<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Platform::bytes(self.0).serialize(serializer)
    }
}

pub mod path {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};

    use super::{Name, Names as _, Platform};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        Name(path.as_os_str()).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        Vec::<u8>::deserialize(deserializer).map(Platform::path)
    }
}

pub mod optional_path {
    use std::path::PathBuf;

    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};

    use super::{Name, Names as _, Platform};

    pub fn serialize<S: Serializer>(
        path: &Option<PathBuf>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        path.as_deref()
            .map(|path| Name(path.as_os_str()))
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PathBuf>, D::Error> {
        Option::<Vec<u8>>::deserialize(deserializer).map(|bytes| bytes.map(Platform::path))
    }
}

pub mod os_string {
    use std::ffi::{OsStr, OsString};

    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};

    use super::{Name, Names as _, Platform};

    pub fn serialize<S: Serializer>(name: &OsStr, serializer: S) -> Result<S::Ok, S::Error> {
        Name(name).serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<OsString, D::Error> {
        Vec::<u8>::deserialize(deserializer).map(Platform::os_string)
    }
}

pub mod os_strings {
    use std::ffi::OsString;

    use serde::{Deserialize as _, Deserializer, Serializer};

    use super::{Name, Names as _, Platform};

    pub fn serialize<S: Serializer>(names: &[OsString], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(names.iter().map(|name| Name(name)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<OsString>, D::Error> {
        Vec::<Vec<u8>>::deserialize(deserializer)
            .map(|names| names.into_iter().map(Platform::os_string).collect())
    }
}

pub mod os_pairs {
    use std::ffi::OsString;

    use serde::{Deserialize as _, Deserializer, Serializer};

    use super::{Name, Names as _, Platform};

    pub fn serialize<S: Serializer>(
        pairs: &[(OsString, OsString)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(pairs.iter().map(|(key, value)| (Name(key), Name(value))))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(OsString, OsString)>, D::Error> {
        Vec::<(Vec<u8>, Vec<u8>)>::deserialize(deserializer).map(|pairs| {
            pairs
                .into_iter()
                .map(|(key, value)| (Platform::os_string(key), Platform::os_string(value)))
                .collect()
        })
    }
}
