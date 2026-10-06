//! How a board is laid out on disk: a directory per board, `board.toml` for
//! the board and its columns, and one `cards/<card-id>.md` per card.
//!
//! `board.toml` holds each column's cards as an ordered list of card ids. A
//! card file is TOML frontmatter between `+++` lines (handle, status,
//! session), then the card's text. The card's id is its file's stem.

use crate::board::{Board, Card, Column, Status, View};
use serde::{Deserialize, Serialize};

pub const BOARD_FILE: &str = "board.toml";
pub const CARDS: &str = "cards";
const FENCE: &str = "+++";

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

#[derive(Default, Serialize, Deserialize)]
struct Front {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
}

/// `board.toml` for a board: everything but the cards' contents.
pub fn board_toml(board: &Board) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(&Disk {
        id: board.id.clone(),
        archived: board.archived,
        name: board.name.clone(),
        key: board.key.clone(),
        next_handle: board.next_handle,
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

/// A card file's contents.
pub fn card_md(card: &Card) -> Result<String, toml::ser::Error> {
    let front = toml::to_string(&Front {
        handle: card.handle,
        status: card.status,
        session: card.session.clone(),
    })?;
    Ok(format!("{FENCE}\n{front}{FENCE}\n{}", card.text))
}

/// A card from its file's contents and its id.
pub fn parse_card(id: &str, body: &str) -> Option<Card> {
    let rest = body.strip_prefix(FENCE)?.strip_prefix('\n')?;
    let (front, text) = match rest.split_once(&format!("\n{FENCE}\n")) {
        Some((front, text)) => (front, text),
        None => (rest.strip_suffix(&format!("\n{FENCE}"))?, ""),
    };
    let front: Front = match front.trim().is_empty() {
        true => Front::default(),
        false => toml::from_str(front).ok()?,
    };
    Some(Card {
        id: id.to_owned(),
        handle: front.handle,
        text: text.to_owned(),
        session: front.session,
        status: front.status,
        version: None,
    })
}

/// A board from `board.toml` and the cards read beside it. Cards no column
/// lists go to the end of the first column; ids with no card are skipped.
pub fn assemble(body: &str, mut cards: Vec<Card>) -> Option<Board> {
    let disk: Disk = toml::from_str(body).ok()?;
    let mut board = Board::new(disk.id, &disk.name);
    board.archived = disk.archived;
    board.key = disk.key;
    board.next_handle = disk.next_handle;
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
