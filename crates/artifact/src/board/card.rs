//! A card: what needs doing, the session that last held it, whether someone is
//! on it, its label and the cards it waits on.

use serde::{Deserialize, Serialize};

/// The one word a card's status is written with — see [`Card::busy`].
pub const BUSY: &str = "busy";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// What names this card, for as long as it exists. Defaulted, and minted
    /// by the read that finds it missing — see [`super::Board::mint_ids`].
    #[serde(default)]
    pub id: String,
    /// The `12` in `ROAD-12`: short enough to say to an agent, and so not
    /// unique for all time the way [`Card::id`] is. Minted on read when
    /// missing — see [`super::Board::mint_ids`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<u64>,
    pub text: String,
    /// The session that last held this card, by the id it is filed under: the
    /// one that made it, was last sent it, or last tagged it busy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Someone is working on it now, as whoever is doing the work says.
    /// Written as `status = "busy"`; any other word there reads as not busy.
    #[serde(
        default,
        rename = "status",
        skip_serializing_if = "std::ops::Not::not",
        with = "busy"
    )]
    pub busy: bool,
    /// Its one label, normalised — see [`crate::label::normalize`]. Drawn from
    /// the labels on its board's cards, not the entries'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The cards it waits on, each a reference as written in its project —
    /// `DEV-12`, or `foo#DEV-12` for a card in project `foo`; see
    /// [`crate::reference`]. One that no longer resolves names nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    /// What the backend held for this card when it was read — see
    /// [`super::Board::version`]. `None` for a card not read from a backend.
    #[serde(skip)]
    pub version: Option<String>,
}

impl Card {
    /// Under an id and a handle its board has found free — see
    /// [`super::Board::add_card`].
    pub fn new(id: String, handle: u64, text: String) -> Self {
        Self {
            id,
            handle: Some(handle),
            text,
            session: None,
            busy: false,
            label: None,
            blocked_by: Vec::new(),
            version: None,
        }
    }
}

/// [`Card::busy`] as the word it is written under.
mod busy {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(_: &bool, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(super::BUSY)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
        Ok(Option::<String>::deserialize(deserializer)?.as_deref() == Some(super::BUSY))
    }
}
