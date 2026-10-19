//! What more than one of this crate's Cupola services carries:
//! [`Refusal`], and fields carried as the bytes of their JSON.

use crate::error::Error;
use serde::{Deserialize, Serialize};

/// A refused call: the [`Error`] it failed with, as far as it crosses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    Stale,
    Gone {
        named: String,
    },
    KeyEmpty,
    KeyTaken {
        key: String,
    },
    /// Any other error, in its own words.
    Other(String),
}

impl From<Error> for Refusal {
    fn from(error: Error) -> Self {
        match error {
            Error::Stale => Self::Stale,
            Error::Gone { named } => Self::Gone { named },
            Error::KeyEmpty => Self::KeyEmpty,
            Error::KeyTaken { key } => Self::KeyTaken { key },
            other => Self::Other(other.to_string()),
        }
    }
}

/// An [`Error`] carried in `anyhow` crosses as that error.
impl From<anyhow::Error> for Refusal {
    fn from(error: anyhow::Error) -> Self {
        Error::from(error).into()
    }
}

impl From<Refusal> for Error {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::Stale => Self::Stale,
            Refusal::Gone { named } => Self::Gone { named },
            Refusal::KeyEmpty => Self::KeyEmpty,
            Refusal::KeyTaken { key } => Self::KeyTaken { key },
            Refusal::Other(message) => Self::Other(anyhow::Error::msg(message)),
        }
    }
}

/// A field carried as the bytes of its JSON.
pub(crate) mod json {
    use serde::{Deserialize as _, Deserializer, Serialize, Serializer, de::DeserializeOwned};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &T,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serde_json::to_vec(value)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }

    pub fn deserialize<'de, T: DeserializeOwned, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<T, D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        serde_json::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}
