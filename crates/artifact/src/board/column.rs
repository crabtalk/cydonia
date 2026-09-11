//! A column: a name, and the cards under it in the order they sit.

use crate::board::Card;
use serde::{Deserialize, Serialize};

/// What a lane is called before you name it.
pub const NAMED: &str = "Column";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// What names this column, for as long as it exists — what an agent holds
    /// while the user renames the column under it. Defaulted, and minted by
    /// the read that finds it missing — see [`super::Board::mint_ids`].
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cards: Vec<Card>,
}

impl Column {
    /// Under an id its board has found free — see
    /// [`super::Board::add_column`].
    pub fn new(id: String, name: &str) -> Self {
        Self {
            id,
            name: name.to_owned(),
            cards: Vec::new(),
        }
    }
}
