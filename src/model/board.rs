//! A project's board: columns of task cards, and the file that outlives them.
//!
//! Machine-written like [`crate::model::state`] — one file holding every
//! project's board, keyed by the path that owns it.
//!
//! Cards nest inside their column, so a `Vec` position *is* the order and a
//! move is a remove and an insert. A flat list with an ordinal only earns its
//! keep where several views group the same cards differently.

use crate::model::{project::Project, settings};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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

fn path() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("boards.toml"))
}

fn stored() -> BTreeMap<PathBuf, Board> {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| toml::from_str(&body).ok())
        .unwrap_or_default()
}

/// This project's board, or a fresh one for a project that has never had one.
pub fn load(project: &Path) -> Board {
    stored().remove(project).unwrap_or_default()
}

/// Best effort, and a merge: the file also holds boards for projects that are
/// not open, and closing a tab must not erase its work.
pub fn save(projects: &[Project]) {
    let Some(path) = path() else {
        return;
    };
    let mut boards = stored();
    for project in projects {
        boards.insert(project.path.clone(), project.board.clone());
    }
    if let Ok(body) = toml::to_string_pretty(&boards)
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&path, body);
    }
}
