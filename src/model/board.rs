//! A project's board: columns of task cards, and the file that outlives them.
//!
//! Machine-written like [`crate::model::state`], and kept in the project's own
//! `.cydonia/` — a board is the project's work, so it travels with the
//! directory rather than living under a path in the config dir that a rename
//! would orphan.
//!
//! Cards nest inside their column, so a `Vec` position *is* the order and a
//! move is a remove and an insert. A flat list with an ordinal only earns its
//! keep where several views group the same cards differently.

use crate::model::project;
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "board.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    #[serde(default)]
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(default)]
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub text: String,
    /// The session this card was dispatched to, this run. Session ids are
    /// minted per launch and nothing resumes across one, so it never persists.
    #[serde(skip)]
    pub session: Option<u64>,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            columns: ["Todo", "Doing", "Done"].map(Column::new).into(),
        }
    }
}

impl Board {
    pub fn column(&self, ix: usize) -> Option<&Column> {
        self.columns.get(ix)
    }

    pub fn card(&self, at: Spot) -> Option<&Card> {
        self.column(at.column)?.cards.get(at.card)
    }

    pub fn card_mut(&mut self, at: Spot) -> Option<&mut Card> {
        self.columns.get_mut(at.column)?.cards.get_mut(at.card)
    }

    /// Lift a card out, for a caller about to put it back somewhere else.
    pub fn take(&mut self, at: Spot) -> Option<Card> {
        let column = self.columns.get_mut(at.column)?;
        (at.card < column.cards.len()).then(|| column.cards.remove(at.card))
    }

    /// Best effort: a board that cannot be written is not worth failing a
    /// click over.
    pub fn save(&self, project: &Path) {
        let Ok(dir) = project::init(project) else {
            return;
        };
        if let Ok(body) = toml::to_string_pretty(self) {
            let _ = std::fs::write(dir.join(FILE), body);
        }
    }
}

impl Column {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            cards: Vec::new(),
        }
    }
}

impl Card {
    pub fn new(text: String) -> Self {
        Self {
            text,
            session: None,
        }
    }
}

/// Where a card sits: its column, and its place in that column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub column: usize,
    pub card: usize,
}

impl Spot {
    pub fn new(column: usize, card: usize) -> Self {
        Self { column, card }
    }
}

/// This project's board, or a fresh one for a project that has never had one.
pub fn load(project: &Path) -> Board {
    std::fs::read_to_string(project::dir(project).join(FILE))
        .ok()
        .and_then(|body| toml::from_str(&body).ok())
        .unwrap_or_default()
}
