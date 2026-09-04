//! A project's boards: columns of task cards, and the files that outlive them.
//!
//! Machine-written like [`crate::model::state`], and kept in the project's own
//! `.cydonia/` — a board is the project's work, so it travels with the
//! directory rather than living under a path in the config dir that a rename
//! would orphan.
//!
//! One file per board, named for the millisecond it was made. An id rather
//! than the name: the name is a property, and a file named after it would be a
//! second copy of it that a refused rename could leave disagreeing.
//!
//! Cards nest inside their column, so a `Vec` position *is* the order and a
//! move is a remove and an insert. A flat list with an ordinal only earns its
//! keep where several views group the same cards differently.

use crate::model::project;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
};

/// Where a project's boards live, and what the one board a project used to be
/// allowed was called.
const DIR: &str = "boards";
const FILE: &str = "board.toml";

/// What a board is called before it is named, and what one whose name has been
/// taken off is shown as.
const NAMED: &str = "Board";
pub const UNNAMED: &str = "Untitled";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    /// The file this board is, which is where [`Board::save`] writes it back.
    #[serde(skip)]
    pub path: PathBuf,
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
    /// The lanes every board starts with.
    fn new(path: PathBuf, name: &str) -> Self {
        Self {
            path,
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

    /// Best effort: a board that cannot be written is not worth failing a
    /// click over.
    pub fn save(&self) {
        if let Ok(body) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&self.path, body);
        }
    }

    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
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

/// This project's boards, most recently written first.
pub fn list(project: &Path) -> Vec<Board> {
    let dir = project::dir(project);
    migrate(&dir);
    let Ok(entries) = std::fs::read_dir(dir.join(DIR)) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort_by_key(|path| Reverse(project::written(path)));
    paths.into_iter().filter_map(read).collect()
}

pub fn create(project: &Path) -> Option<Board> {
    let dir = project::init(project).ok()?.join(DIR);
    std::fs::create_dir_all(&dir).ok()?;
    let board = Board::new(free(&dir, project::stamp()), NAMED);
    board.save();
    Some(board)
}

fn read(path: PathBuf) -> Option<Board> {
    let body = std::fs::read_to_string(&path).ok()?;
    let mut board: Board = toml::from_str(&body).ok()?;
    board.path = path;
    Some(board)
}

/// This millisecond's file, or the first after it that is not taken. Two boards
/// made inside one millisecond is the only way that happens.
fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(format!("{stamp}.toml")))
        .find(|board| !board.exists())
        .unwrap_or_else(|| dir.join(format!("{stamp}.toml")))
}

/// A project used to have one board, in `.cydonia/board.toml`. Give it the
/// directory and the name the rest are made with, and it is the first of many.
fn migrate(dir: &Path) {
    let old = dir.join(FILE);
    let Some(mut board) = read(old.clone()) else {
        return;
    };
    let to = dir.join(DIR);
    if std::fs::create_dir_all(&to).is_err() {
        return;
    }
    board.path = free(&to, project::stamp());
    board.name = NAMED.to_owned();
    board.save();
    let _ = std::fs::remove_file(old);
}
