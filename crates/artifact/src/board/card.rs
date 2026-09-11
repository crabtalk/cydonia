//! A card: what needs doing, and the session it was handed to.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// What names this card, for as long as it exists. Defaulted, and minted
    /// by the read that finds it missing — see [`super::Board::mint_ids`].
    #[serde(default)]
    pub id: String,
    pub text: String,
    /// The session this card was dispatched to, by the id that session is
    /// filed under.
    ///
    /// Persisted, unlike the run-local number it used to be: sessions are read
    /// back from disk, so a link that died at quit left the board offering to
    /// dispatch a card that already had an agent on it — and dispatching again
    /// started a second one at the same task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Card {
    /// Under an id its board has found free — see [`super::Board::add_card`],
    /// which is the only thing that knows what is taken.
    pub fn new(id: String, text: String) -> Self {
        Self {
            id,
            text,
            session: None,
        }
    }
}
