//! A column: a name, and the cards under it in the order they sit.

use crate::board::Card;
use serde::{Deserialize, Serialize};

/// What a lane is called before you name it.
pub const NAMED: &str = "COLUMN";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// What an agent holds while the column is renamed under it. Minted on
    /// read when missing — see [`super::Board::mint_ids`].
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// Folded shut in the list view, where a lane is a section of one long
    /// column and a long one buries the lanes under it. The lanes view ignores
    /// it.
    ///
    /// Ahead of the cards: TOML takes no value after an array of tables, so a
    /// scalar written under them would not round-trip.
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub cards: Vec<Card>,
}

/// What a lane is called, in the one case a board writes: upper. Applied where
/// a name is taken in rather than where it is drawn, so the file holds the name
/// the board shows and an agent reading it back sees the same string.
pub fn heading(name: &str) -> String {
    name.to_uppercase()
}

impl Column {
    /// Under an id its board has found free — see
    /// [`super::Board::add_column`].
    pub fn new(id: String, name: &str) -> Self {
        Self {
            id,
            name: heading(name),
            collapsed: false,
            cards: Vec::new(),
        }
    }
}
