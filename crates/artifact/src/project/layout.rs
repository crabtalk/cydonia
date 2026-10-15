//! How a board is laid out on disk: a directory per board, `board.toml` for
//! the board and its columns, and a directory per card, `cards/<card-id>/`.
//!
//! `board.toml` holds each column's cards as an ordered list of card ids. A
//! card's directory is a [`crate::document`]: `content.md` is its text,
//! `properties.toml` its handle, status, session, label, the cards it waits
//! on and the cards waiting on it, and `assets/` its pictures, referenced as
//! `assets/<name>`. The card's id is its directory's
//! name. The board directory is a document too: its `content.md` is the
//! board's description, absent for none.

use crate::{
    board::{Board, Card, Column, View, card::BUSY},
    document,
};
use serde::{Deserialize, Serialize};

pub const BOARD_FILE: &str = "board.toml";
pub const CARDS: &str = "cards";

const HANDLE: &str = "handle";
const STATUS: &str = "status";
const SESSION: &str = "session";
const LABEL: &str = "label";
const BLOCKED_BY: &str = "blocked_by";
const BLOCKS: &str = "blocks";

#[derive(Serialize, Deserialize)]
struct Disk {
    #[serde(default)]
    id: String,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    next_handle: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<String>,
    #[serde(default)]
    view: View,
    #[serde(default)]
    columns: Vec<DiskColumn>,
}

#[derive(Serialize, Deserialize)]
struct DiskColumn {
    #[serde(default)]
    id: String,
    name: String,
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    cards: Vec<String>,
}

/// `board.toml` for a board: everything but the cards' contents.
pub fn board_toml(board: &Board) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(&Disk {
        id: board.id.clone(),
        archived: board.archived,
        name: board.name.clone(),
        key: board.key.clone(),
        next_handle: board.next_handle,
        labels: board.labels.clone(),
        view: board.view,
        columns: board
            .columns
            .iter()
            .map(|column| DiskColumn {
                id: column.id.clone(),
                name: column.name.clone(),
                collapsed: column.collapsed,
                cards: column.cards.iter().map(|card| card.id.clone()).collect(),
            })
            .collect(),
    })
}

/// The card ids `board.toml` lists, in any column. Empty for a body that does
/// not parse.
pub fn card_ids(body: &str) -> std::collections::HashSet<String> {
    toml::from_str::<Disk>(body)
        .map(|disk| {
            disk.columns
                .into_iter()
                .flat_map(|column| column.cards)
                .collect()
        })
        .unwrap_or_default()
}

/// A card's `properties.toml`, written over `held` (what the file holds now)
/// so keys cydonia does not know about stay. `None` for a card with nothing to
/// say there, whose file should not exist.
pub fn card_properties(card: &Card, held: &str) -> Option<String> {
    document::apply(
        held,
        &[
            (
                HANDLE,
                card.handle
                    .and_then(|handle| i64::try_from(handle).ok())
                    .map(toml_edit::value),
            ),
            (STATUS, card.busy.then(|| toml_edit::value(BUSY))),
            (SESSION, card.session.as_deref().map(toml_edit::value)),
            (LABEL, card.label.as_deref().map(toml_edit::value)),
            (BLOCKED_BY, references(&card.blocked_by)),
            (BLOCKS, references(&card.blocks)),
        ],
    )
}

/// A card from its directory's name, its `content.md` and its
/// `properties.toml`.
pub fn parse_card(id: &str, content: &str, properties: &str) -> Card {
    let doc = document::parse(properties);
    Card {
        id: id.to_owned(),
        handle: doc
            .get(HANDLE)
            .and_then(|handle| handle.as_integer())
            .and_then(|handle| u64::try_from(handle).ok()),
        text: content.to_owned(),
        session: doc
            .get(SESSION)
            .and_then(|session| session.as_str())
            .map(str::to_owned),
        busy: doc
            .get(STATUS)
            .and_then(|status| status.as_str())
            .is_some_and(|status| status == BUSY),
        label: doc
            .get(LABEL)
            .and_then(|label| label.as_str())
            .and_then(crate::label::normalize),
        blocked_by: read_references(&doc, BLOCKED_BY),
        blocks: read_references(&doc, BLOCKS),
        version: None,
    }
}

/// A list of card references as `properties.toml` holds it; none for an empty
/// one.
fn references(list: &[String]) -> Option<toml_edit::Item> {
    (!list.is_empty()).then(|| toml_edit::value(list.iter().collect::<toml_edit::Array>()))
}

fn read_references(doc: &toml_edit::DocumentMut, key: &str) -> Vec<String> {
    doc.get(key)
        .and_then(|ids| ids.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A board from `board.toml` and the cards read beside it. Cards no column
/// lists go to the end of the first column; ids with no card are skipped.
pub fn assemble(body: &str, mut cards: Vec<Card>) -> Option<Board> {
    let disk: Disk = toml::from_str(body).ok()?;
    let mut board = Board::new(disk.id, &disk.name);
    board.archived = disk.archived;
    board.key = disk.key;
    board.next_handle = disk.next_handle;
    board.labels = disk.labels;
    board.view = disk.view;
    for column in disk.columns {
        board.columns.push(Column {
            cards: column
                .cards
                .iter()
                .filter_map(|id| {
                    let at = cards.iter().position(|card| &card.id == id)?;
                    Some(cards.swap_remove(at))
                })
                .collect(),
            id: column.id,
            name: column.name,
            collapsed: column.collapsed,
        });
    }
    if !cards.is_empty() {
        cards.sort_by(|a, b| a.handle.cmp(&b.handle).then_with(|| a.id.cmp(&b.id)));
        match board.columns.first_mut() {
            Some(first) => first.cards.extend(cards),
            None => {
                let mut column = Column::new(String::new(), crate::board::column::NAMED);
                column.cards = cards;
                board.columns.push(column);
            }
        }
    }
    Some(board)
}
