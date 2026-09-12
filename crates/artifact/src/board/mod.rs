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

/// The number the first card on a board takes. One rather than nought, because
/// a handle is read by a person.
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
    /// What every card here is prefixed with — `ROAD`, for `ROAD-12`. Derived
    /// from the name when the board is made and kept across a rename: a handle
    /// that has been said out loud has to go on meaning the card it meant.
    ///
    /// Unique among the boards of one project, which is as far as a handle ever
    /// has to carry — the agent that hears one is running in that project.
    #[serde(default)]
    pub key: String,
    /// The number the next card here takes. Kept in the file rather than read
    /// back as `max + 1` over the cards: the counter only climbs, so a deleted
    /// ROAD-12 leaves a gap instead of coming back as somebody else's.
    #[serde(default)]
    pub next_handle: u64,
    #[serde(default)]
    pub columns: Vec<Column>,
}

impl Board {
    /// A board with nothing on it, under the name the backend about to keep it
    /// has minted.
    ///
    /// No lanes: Todo/Doing/Done here was a guess about the work compiled into
    /// a constructor, and a board whose columns you name has to start with the
    /// ones you named.
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

    /// Give every column and card that has none an id, and say whether
    /// anything was given one — which is what tells the backend that what it
    /// just read is behind what it now holds.
    ///
    /// The board a caller gets back is addressable or it is nothing: a client
    /// handed a card with no id cannot name it to say anything about it.
    pub fn mint_ids(&mut self) -> bool {
        let mut taken = self.taken();
        // Handles come off the counter even here, so a board read twice never
        // numbers the same card differently — and a card that had one keeps it.
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

    /// Every id this board is already using. Unique within the board and no
    /// further — nothing outside a board points at a card.
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
    // Named operations rather than reaching into `columns`, because this is
    // the vocabulary the MCP tools answer in: a caller over a connection has
    // only what is named here, and the pane taking the same route is what
    // keeps the two from drifting.

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

    /// Drop a lane, and refuse while it still holds cards.
    ///
    /// The cards are the work and the column is only where they sit, so there
    /// is no reading of "delete this column" that means "and the work in it".
    /// Emptying it first is a step; a tool call that quietly took six cards
    /// with it is not recoverable.
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

    /// The next number, and the counter moved past it. Answers [`FIRST`] for a
    /// board written before the counter existed, whose default is zero — a card
    /// numbered nought reads as an error, not as the first of anything.
    fn take_handle(&mut self) -> u64 {
        let handle = self.next_handle.max(FIRST);
        self.next_handle = handle + 1;
        handle
    }

    /// What to call this card out loud: `ROAD-12`. Nothing while the board has
    /// no key, which is a board nobody has written back yet.
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

/// An id nothing on this board is using. `-2` settles a tie the same way a
/// session's file name does — a pass runs well inside the millisecond every id
/// in it is minted from, so ties are the rule and not the exception.
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
