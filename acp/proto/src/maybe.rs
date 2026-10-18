//! Three-state optionality for partial updates.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Distinguishes "field absent" from "field explicitly null".
///
/// A partial update needs all three: leave the value alone, clear it, or set
/// it. Plain `Option` collapses the first two.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MaybeUndefined<T> {
    /// The key was not present — leave the current value alone.
    #[default]
    Undefined,
    /// The key was present and null — clear the current value.
    Null,
    Value(T),
}

impl<T> MaybeUndefined<T> {
    pub fn is_undefined(&self) -> bool {
        matches!(self, Self::Undefined)
    }

    /// `None` covers both "unchanged" and "cleared" for callers that treat
    /// them alike.
    pub fn value(self) -> Option<T> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }
}

impl<T> From<Option<T>> for MaybeUndefined<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => Self::Value(v),
            None => Self::Null,
        }
    }
}

impl<T: Serialize> Serialize for MaybeUndefined<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // Callers pair this with `skip_serializing_if`, so `Undefined`
            // should never reach the serializer; null is the honest fallback.
            Self::Undefined | Self::Null => serializer.serialize_none(),
            Self::Value(v) => serializer.serialize_some(v),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for MaybeUndefined<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(Into::into)
    }
}
