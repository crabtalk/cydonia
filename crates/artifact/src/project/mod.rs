//! A project, as whatever holds it: the boards, sessions and articles in one,
//! the numbers they are referred to by, and the reads and writes over them.
//!
//! [`Project`] is what a backend answers, and [`fs`] is the one cydonia ships
//! — files under the project's own `.cydonia/`. The shapes themselves name no
//! file and open none, which is what lets a board be the same board whichever
//! backend handed it over.
//!
//! Everything is named by id. A path is a detail of [`fs`] and never crosses
//! this trait.

pub mod fs;

use crate::{
    article::{Article, properties::Properties},
    board::Board,
    session::record::Record,
};
use anyhow::Result;
use std::sync::Arc;

/// A watch a backend keeps up for as long as this is held.
pub struct Watching {
    /// Whether it is on everything the backend reads back. `false` for one on
    /// a stand-in that only knocks when the real thing can be watched, which
    /// is when the caller asks again.
    pub settled: bool,
    _guard: Box<dyn Send>,
}

impl Watching {
    pub fn new(settled: bool, guard: impl Send + 'static) -> Self {
        Self {
            settled,
            _guard: Box::new(guard),
        }
    }
}

pub trait Project {
    // ── boards ───────────────────────────────────────────────────────

    /// This project's boards, most recently written first.
    fn boards(&self) -> Vec<Board>;

    /// One board, without reading the rest of the project.
    fn board(&self, id: &str) -> Option<Board>;

    /// Mint a board and file it, called and keyed as the caller has them.
    /// An empty key is derived from the name, clear of the keys the project's
    /// other boards hold — see [`crate::board::key`].
    fn create_board(&self, name: &str, key: &str) -> Result<Board>;

    /// Write a board back, and take the time it was written at — the key the
    /// sidebar orders on, which only the backend knows.
    fn save_board(&self, board: &mut Board) -> Result<()>;

    /// Take a board out, and retire its number.
    fn remove_board(&self, id: &str) -> Result<()>;

    // ── sessions ─────────────────────────────────────────────────────

    /// Every session filed here, most recently updated first.
    fn sessions(&self) -> Vec<Record>;

    /// One session, without reading the rest of the project.
    fn session(&self, id: &str) -> Option<Record>;

    /// Mint the id a session is filed under from here on. Called on its first
    /// write and not before: opening a project must not put anything in it.
    fn create_session(&self) -> Result<String>;

    fn save_session(&self, record: &Record) -> Result<()>;

    /// Take a session out, and retire its number.
    fn remove_session(&self, id: &str) -> Result<()>;

    // ── articles ─────────────────────────────────────────────────────

    /// Every article filed here, most recently touched first. Bodies are not
    /// read.
    fn articles(&self) -> Vec<Article>;

    /// Mint an article holding this markdown, with no properties.
    fn create_article(&self, markdown: &str) -> Result<Article>;

    /// An article's markdown.
    fn read_article(&self, id: &str) -> Result<String>;

    fn write_article(&self, id: &str, markdown: &str) -> Result<()>;

    /// What an article holds beside its markdown. Default for one that has
    /// none.
    fn properties(&self, id: &str) -> Properties;

    /// Write the properties back. Keys this crate does not know about are
    /// kept.
    fn save_properties(&self, id: &str, properties: &Properties) -> Result<()>;

    /// Take an article out, its assets and cover with it, and retire its
    /// number.
    fn remove_article(&self, id: &str) -> Result<()>;

    /// One of the pictures in an article's body, by the name it was filed
    /// under.
    fn asset(&self, id: &str, name: &str) -> Result<Vec<u8>>;

    fn put_asset(&self, id: &str, name: &str, bytes: &[u8]) -> Result<()>;

    // ── changes ──────────────────────────────────────────────────────

    /// Call `knock` whenever something this backend reads back changes under
    /// it, from any thread. A knock carries nothing: the answer to one is a
    /// re-read. `None` for a backend that has nothing to watch or cannot.
    fn watch(&self, knock: Arc<dyn Fn() + Send + Sync>) -> Option<Watching> {
        let _ = knock;
        None
    }

    // ── numbers ──────────────────────────────────────────────────────

    /// The project-wide number an entry is referred to by (`#12`), issued on
    /// first ask. Every client of one project must get the same answer.
    fn number(&self, kind: &str, id: &str) -> Result<u64>;

    /// The id a number was issued to, or `None` for one never issued or
    /// retired.
    fn resolve(&self, kind: &str, number: u64) -> Result<Option<String>>;

    /// Retire an entry's number. It is never issued again.
    fn retire(&self, kind: &str, id: &str) -> Result<()>;
}
