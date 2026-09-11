//! A project's boards: columns of task cards, and the files that outlive them.
//!
//! Machine-written, and kept in the project's own `.cydonia/` — a board is the
//! project's work, so it travels with the directory rather than living under a
//! path in the config dir that a rename would orphan.
//!
//! One file per board, named for the millisecond it was made. An id rather
//! than the name: the name is a property, and a file named after it would be a
//! second copy of it that a refused rename could leave disagreeing.
//!
//! Cards nest inside their column, so a `Vec` position *is* the order and a
//! move is a remove and an insert. A flat list with an ordinal only earns its
//! keep where several views group the same cards differently.

use crate::stamp;
use serde::{Deserialize, Serialize};

/// What a board is called before it is named, and what one whose name has been
/// taken off is shown as.
pub const NAMED: &str = "Board";
pub const UNNAMED: &str = "Untitled";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    /// What names this board, for as long as it exists. Written into the file
    /// rather than left to be the file's name: a board a backend keeps in a
    /// row has no name to be, and a reader that was handed one has no
    /// directory to look in.
    ///
    /// Defaulted, and filled from the file's own name when a board written
    /// before ids existed is read — see [`crate::id`].
    #[serde(default)]
    pub id: String,
    /// When it was last written, as the backend that holds it counts — kept
    /// beside the board because the sidebar orders on it, and never written
    /// into the record, which would be a second copy able to disagree.
    #[serde(skip)]
    pub touched: u128,
    /// Put away: listed under the divider rather than gone.
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub name: String,
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

impl Board {
    /// The lanes every board starts with, under the name the backend about to
    /// keep it has minted.
    pub fn new(id: String, name: &str) -> Self {
        Self {
            id,
            touched: stamp::now(),
            archived: false,
            name: name.to_owned(),
            columns: ["Todo", "Doing", "Done"].map(Column::new).into(),
        }
    }

    /// The sidebar's label.
    pub fn label(&self) -> &str {
        match self.name.is_empty() {
            true => UNNAMED,
            false => &self.name,
        }
    }

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

    /// Take what a re-read of the project found, which is what the app answers a
    /// change under `.cydonia/` with.
    ///
    /// Nothing here is unsaved: a board is written on the click that changes
    /// it, so the file is always the board and taking it whole is safe. What is
    /// worth avoiding is taking it *needlessly* — a card's link to the session
    /// it opened is [`serde`]-skipped, so it would go with every echo of our
    /// own save. Compared as the file rather than by its stamp for that reason.
    ///
    /// Answers whether the board was actually replaced, which is what tells a
    /// pane holding a card's position that the position is not that card's any
    /// more.
    pub fn adopt(&mut self, fresh: Self) -> bool {
        if toml::to_string_pretty(self).ok() == toml::to_string_pretty(&fresh).ok() {
            self.touched = fresh.touched;
            return false;
        }
        *self = fresh;
        true
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
