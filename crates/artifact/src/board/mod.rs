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

/// How a board's cards are laid out: in lanes across, or in one list down.
///
/// On the board rather than on the pane showing it, so it is written into the
/// file and travels with the project — a board opened in a second window is
/// laid out the way it was left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// Lanes across, a column to each.
    #[default]
    Lanes,
    /// One list down, grouped under its columns.
    List,
}

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
    #[serde(skip)]
    pub number: Option<u64>,
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
    /// How the pane lays this board out — see [`View`].
    ///
    /// Ahead of the columns: TOML takes no value after a table, so a scalar
    /// written under the array of columns would not round-trip.
    #[serde(default)]
    pub view: View,
    #[serde(default)]
    pub columns: Vec<Column>,
}

impl Board {
    /// A board with nothing on it. No lanes: Todo/Doing/Done here was a guess
    /// about the work compiled into a constructor.
    pub fn new(id: String, name: &str) -> Self {
        Self {
            id,
            number: None,
            touched: stamp::now(),
            archived: false,
            name: name.to_owned(),
            // Filled by whoever knows what the neighbouring boards have taken
            // — see [`key::derive`]. Empty until then, the way the ids are.
            key: String::new(),
            next_handle: FIRST,
            view: View::default(),
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
                column.name = column::heading(name);
                true
            }
            None => false,
        }
    }

    /// Put a lane in front of another, or at the right-hand end with no anchor
    /// — [`Self::move_card_before`]'s shape, and for the same reason: an index
    /// means whatever the board looked like when it was counted, and a board is
    /// written by a pane and a tool at once.
    ///
    /// A lane asked to go in front of itself stays where it is.
    pub fn move_column_before(&mut self, id: &str, before: Option<&str>) -> bool {
        if before == Some(id) {
            return false;
        }
        let Some(from) = self.columns.iter().position(|column| column.id == id) else {
            return false;
        };
        let column = self.columns.remove(from);
        let at = before
            .and_then(|before| self.columns.iter().position(|column| column.id == before))
            .unwrap_or(self.columns.len());
        self.columns.insert(at, column);
        true
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

    /// The lane a card is sitting in.
    pub fn column_of(&self, card: &str) -> Option<&Column> {
        self.columns
            .iter()
            .find(|column| column.cards.iter().any(|held| held.id == card))
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
        self.move_card_before(id, to, None)
    }

    /// The same move, landing in front of `before` rather than at the end.
    ///
    /// An anchor card rather than a position, because a position means
    /// whatever the lane looked like when it was counted: lifting the card out
    /// renumbers everything under it, a re-read can renumber the rest, and the
    /// drop lands beside a different card than the one it was aimed at. The
    /// card it goes in front of is that card however the lane moved.
    ///
    /// An anchor the lane does not hold lands at the end, which is what a drop
    /// with nothing under it already means. The card itself as the anchor is
    /// the one refusal: a card asked to go in front of itself is a card
    /// dropped where it already is, and putting it at the end instead would
    /// make the shortest drag on the board the furthest move.
    pub fn move_card_before(&mut self, id: &str, to: &str, before: Option<&str>) -> bool {
        if before == Some(id) || self.column(to).is_none() {
            return false;
        }
        let Some(card) = self.remove_card(id) else {
            return false;
        };
        match self.column_mut(to) {
            Some(column) => {
                let at = before
                    .and_then(|before| column.cards.iter().position(|card| card.id == before))
                    .unwrap_or(column.cards.len());
                column.cards.insert(at, card);
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
            self.number = fresh.number;
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

// ── moving a card to another board ───────────────────────────────

/// Carry a card from one board to another, which may be in another project.
///
/// The card arrives under a fresh id and a handle off the destination's own
/// counter, so a card that was `ROAD-12` is `PLAN-3` from here on. Anything
/// that already said `ROAD-12` still says it.
///
/// Its session is left behind — see [`Card::session`].
///
/// Lands in `column` where one is named, otherwise in the lane of the same name
/// as the one it came out of, otherwise in the first lane. A board with no lanes
/// answers `None`, with the card where it was.
pub fn carry_card(
    from: &mut Board,
    to: &mut Board,
    card: &str,
    column: Option<&str>,
) -> Option<String> {
    let source = from.column_of(card)?;
    let (came_from, named) = (source.id.clone(), source.name.clone());
    let landing = column
        .and_then(|named| to.column(named).map(|column| column.id.clone()))
        .or_else(|| {
            to.columns
                .iter()
                .find(|column| column.name == named)
                .map(|column| column.id.clone())
        })
        .or_else(|| to.columns.first().map(|column| column.id.clone()))?;
    let mut card = from.remove_card(card)?;
    card.id = to.mint_id();
    card.handle = Some(to.take_handle());
    card.session = None;
    let handle = to.handle_of(&card);
    let id = card.id.clone();
    match to.column_mut(&landing) {
        Some(column) => column.cards.push(card),
        // Unreachable: the lane was here when it was chosen. Putting the card
        // back where it came from rather than dropping it on the floor.
        None => {
            from.add_card(&came_from, card.text.clone());
            return None;
        }
    }
    Some(handle.unwrap_or(id))
}
