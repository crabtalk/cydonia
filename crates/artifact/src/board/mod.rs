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

pub mod card;
pub mod column;
pub mod key;

pub use card::Card;
pub use column::Column;

use crate::{id, stamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// What a board is called before it is named, and what one whose name has been
/// taken off is shown as.
pub const NAMED: &str = "Board";

/// The number the first card takes. One, not nought: a person reads it.
pub const FIRST: u64 = 1;
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
    /// What every card here is prefixed with — `ROAD`, for `ROAD-12`. Unique
    /// among one project's boards, which is as far as a handle has to carry.
    /// See [`key`].
    #[serde(default)]
    pub key: String,
    /// The number the next card takes. In the file rather than `max + 1` over
    /// the cards: it only climbs, so a deleted ROAD-12 leaves a gap instead of
    /// coming back as somebody else's.
    #[serde(default)]
    pub next_handle: u64,
    #[serde(default)]
    pub columns: Vec<Column>,
}

impl Board {
    /// A board with nothing on it. No lanes: Todo/Doing/Done here was a guess
    /// about the work compiled into a constructor.
    pub fn new(id: String, name: &str) -> Self {
        Self {
            id,
            touched: stamp::now(),
            archived: false,
            name: name.to_owned(),
            // Filled by whoever knows what the neighbouring boards have taken
            // — see [`key::derive`]. Empty until then, the way the ids are.
            key: String::new(),
            next_handle: FIRST,
            columns: Vec::new(),
        }
    }

    /// The sidebar's label.
    pub fn label(&self) -> &str {
        match self.name.is_empty() {
            true => UNNAMED,
            false => &self.name,
        }
    }

    /// Name everything that has no name — ids and handles — and say whether
    /// anything was named, which is what tells the backend its file is behind.
    /// A card a client cannot name is a card it cannot say anything about.
    pub fn mint_ids(&mut self) -> bool {
        let mut taken = self.taken();
        // Off the same counter, so a board read twice numbers alike.
        let mut next = self.next_handle.max(FIRST);
        let mut minted = false;
        for column in &mut self.columns {
            if column.id.is_empty() {
                column.id = mint(&mut taken);
                minted = true;
            }
            for card in &mut column.cards {
                if card.id.is_empty() {
                    card.id = mint(&mut taken);
                    minted = true;
                }
                if card.handle.is_none() {
                    card.handle = Some(next);
                    next += 1;
                    minted = true;
                }
            }
        }
        if self.next_handle != next {
            self.next_handle = next;
            minted = true;
        }
        minted
    }

    /// Every id in use here. Unique within the board and no further — nothing
    /// outside one points at a card.
    fn taken(&self) -> HashSet<String> {
        self.columns
            .iter()
            .flat_map(|column| {
                std::iter::once(&column.id).chain(column.cards.iter().map(|card| &card.id))
            })
            .filter(|id| !id.is_empty())
            .cloned()
            .collect()
    }

    /// An id no column or card here holds.
    fn mint_id(&self) -> String {
        mint(&mut self.taken())
    }

    // ── columns ──────────────────────────────────────────────────
    //
    // Named operations rather than reaching into `columns`: this is the
    // vocabulary the MCP tools answer in, and the pane taking the same route is
    // what keeps the two from drifting.

    pub fn column(&self, id: &str) -> Option<&Column> {
        self.columns.iter().find(|column| column.id == id)
    }

    fn column_mut(&mut self, id: &str) -> Option<&mut Column> {
        self.columns.iter_mut().find(|column| column.id == id)
    }

    /// A lane at the right-hand end.
    pub fn add_column(&mut self, name: &str) -> &Column {
        let id = self.mint_id();
        self.columns.push(Column::new(id, name));
        self.columns.last().expect("just pushed")
    }

    pub fn rename_column(&mut self, id: &str, name: &str) -> bool {
        match self.column_mut(id) {
            Some(column) => {
                column.name = name.to_owned();
                true
            }
            None => false,
        }
    }

    /// Drop a lane, refusing while it holds cards: "delete this column" has no
    /// reading that means "and the work in it".
    pub fn remove_column(&mut self, id: &str) -> bool {
        let Some(at) = self
            .columns
            .iter()
            .position(|column| column.id == id && column.cards.is_empty())
        else {
            return false;
        };
        self.columns.remove(at);
        true
    }

    // ── cards ────────────────────────────────────────────────────

    pub fn card(&self, id: &str) -> Option<&Card> {
        self.columns
            .iter()
            .flat_map(|column| column.cards.iter())
            .find(|card| card.id == id)
    }

    pub fn card_mut(&mut self, id: &str) -> Option<&mut Card> {
        self.columns
            .iter_mut()
            .flat_map(|column| column.cards.iter_mut())
            .find(|card| card.id == id)
    }

    /// At the end of the column, which is where a card written into a lane
    /// lands.
    pub fn add_card(&mut self, column: &str, text: String) -> Option<&Card> {
        let id = self.mint_id();
        let handle = self.take_handle();
        let column = self.column_mut(column)?;
        column.cards.push(Card::new(id, handle, text));
        column.cards.last()
    }

    /// The next number, and the counter moved past it. [`FIRST`] for a board
    /// written before the counter existed, since nought reads as an error.
    fn take_handle(&mut self) -> u64 {
        let handle = self.next_handle.max(FIRST);
        self.next_handle = handle + 1;
        handle
    }

    /// What to call this card out loud: `ROAD-12`.
    pub fn handle_of(&self, card: &Card) -> Option<String> {
        let handle = card.handle?;
        (!self.key.is_empty()).then(|| format!("{}-{handle}", self.key))
    }

    pub fn rewrite_card(&mut self, id: &str, text: &str) -> bool {
        match self.card_mut(id) {
            Some(card) => {
                card.text = text.to_owned();
                true
            }
            None => false,
        }
    }

    /// Carry a card to the end of another lane, its session with it.
    pub fn move_card(&mut self, id: &str, to: &str) -> bool {
        if self.column(to).is_none() {
            return false;
        }
        let Some(card) = self.remove_card(id) else {
            return false;
        };
        match self.column_mut(to) {
            Some(column) => {
                column.cards.push(card);
                true
            }
            // Unreachable: `to` was here a moment ago and removing a card
            // cannot drop a column. Putting it back rather than dropping it is
            // what makes that true whether or not it stays true.
            None => self
                .columns
                .first_mut()
                .map(|column| column.cards.push(card))
                .is_some(),
        }
    }

    /// Lift a card out — off the board for good, or on its way elsewhere.
    pub fn remove_card(&mut self, id: &str) -> Option<Card> {
        for column in &mut self.columns {
            if let Some(at) = column.cards.iter().position(|card| card.id == id) {
                return Some(column.cards.remove(at));
            }
        }
        None
    }

    /// Write down the session a card was handed to.
    pub fn dispatch_card(&mut self, id: &str, session: String) -> bool {
        match self.card_mut(id) {
            Some(card) => {
                card.session = Some(session);
                true
            }
            None => false,
        }
    }

    /// Take what a re-read of the project found, which is what the app answers a
    /// change under `.cydonia/` with.
    ///
    /// Nothing here is unsaved: a board is written on the click that changes
    /// it, so the file is always the board and taking it whole is safe. What is
    /// worth avoiding is taking it *needlessly* — every echo of our own save is
    /// one of these, and a pane holding a card's position would be told the
    /// position is not that card's any more for nothing.
    ///
    /// Answers whether the board was actually replaced, which is what tells
    /// that pane.
    pub fn adopt(&mut self, fresh: Self) -> bool {
        if toml::to_string_pretty(self).ok() == toml::to_string_pretty(&fresh).ok() {
            self.touched = fresh.touched;
            return false;
        }
        *self = fresh;
        true
    }
}

/// An id nothing here is using. A pass runs well inside the millisecond its
/// ids are minted from, so `-2` settles the tie — as a session's file name
/// does.
fn mint(taken: &mut HashSet<String>) -> String {
    let stamp = id::mint();
    let mut fresh = stamp.clone();
    let mut n = 2;
    while !taken.insert(fresh.clone()) {
        fresh = format!("{stamp}-{n}");
        n += 1;
    }
    fresh
}
