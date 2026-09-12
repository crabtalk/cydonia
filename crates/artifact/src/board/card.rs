//! A card: what needs doing, and the session it was handed to.

use serde::{Deserialize, Serialize};

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
    /// The session this card was dispatched to, by the id it is filed under.
    /// Persisted, so ▶/💬 still tells the truth after a quit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
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
        }
    }
}
