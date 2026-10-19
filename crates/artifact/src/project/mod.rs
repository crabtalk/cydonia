//! A project, as whatever holds it: the boards, sessions and articles in one,
//! the numbers they are referred to by, and the reads and writes over them.
//!
//! [`Project`] is what a backend answers, and [`fs`] is the one cydonia ships
//! — files under the project's own `.cydonia/`. The shapes themselves name no
//! file and open none, which is what lets a board be the same board whichever
//! backend handed it over.
//!
//! Everything is named by id. The paths that cross this trait are on the host
//! the backend keeps its files on: [`Project::place`] and [`Project::files`].
//!
//! Reads answer at once, but for [`Project::asset`], a fetch answered with a
//! future. A write answers with a future: the write is issued at the call, in
//! call order, and the future resolves when the backend has made it. Dropping
//! the future does not take the write back.

pub mod fs;
pub mod layout;
pub mod memory;
#[cfg(feature = "remote")]
pub mod remote;
#[cfg(feature = "remote")]
pub mod service;
pub mod store;

use crate::kind::Kind;
use crate::{
    article::{Article, comments::Comments, properties::Properties},
    board::{Board, Page},
    session::record::Record,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a [`Project`] answers a write or a lookup with.
pub type Result<T> = std::result::Result<T, crate::error::Error>;

/// A watch a backend keeps up for as long as this is held.
pub struct Watching {
    /// Whether it is on everything the backend reads back. `false` for one on
    /// a stand-in that only knocks when the real thing can be watched, which
    /// is when the caller asks again.
    pub settled: bool,
    _guard: Box<dyn Send>,
}

/// Whose pictures an asset is filed among.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Owner {
    /// An article's, by its id.
    Article(String),
    /// A page of a board: one of its cards, or its description.
    Page { board: String, page: Page },
    /// The project's own: what a session's messages carry, and the pictures
    /// in articles written before each held its own.
    Shared,
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
    fn create_board(
        &self,
        name: &str,
        key: &str,
    ) -> impl Future<Output = Result<Board>> + Send + use<Self>;

    /// Write a board back. The board takes, at the call, the time it was
    /// written at — the key the sidebar orders on, which only the backend
    /// knows — and the version it now has. Refused with
    /// [`crate::error::Error::Stale`] when the board carries a version the
    /// backend no longer holds, or when it is gone.
    fn save_board(&self, board: &mut Board) -> impl Future<Output = Result<()>> + Send + use<Self>;

    /// Take a board out, and retire its number.
    fn remove_board(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<Self>;

    // ── sessions ─────────────────────────────────────────────────────

    /// Every session filed here, most recently updated first.
    fn sessions(&self) -> Vec<Record>;

    /// One session, without reading the rest of the project.
    fn session(&self, id: &str) -> Option<Record>;

    /// Mint the id a session is filed under from here on. Called on its first
    /// write and not before: opening a project must not put anything in it.
    fn create_session(&self) -> impl Future<Output = Result<String>> + Send + use<Self>;

    fn save_session(&self, record: &Record) -> impl Future<Output = Result<()>> + Send + use<Self>;

    /// Take a session out, and retire its number.
    fn remove_session(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<Self>;

    // ── articles ─────────────────────────────────────────────────────

    /// Every article filed here, most recently touched first. Bodies are not
    /// read.
    fn articles(&self) -> Vec<Article>;

    /// One article, without reading the rest of the project.
    fn article(&self, id: &str) -> Option<Article>;

    /// Mint an article holding this markdown, with no properties.
    fn create_article(
        &self,
        markdown: &str,
    ) -> impl Future<Output = Result<Article>> + Send + use<Self>;

    /// An article's markdown.
    fn read_article(&self, id: &str) -> Result<String>;

    /// Write an article's markdown, carrying its comments through the change.
    fn write_article(
        &self,
        id: &str,
        markdown: &str,
    ) -> impl Future<Output = Result<()>> + Send + use<Self>;

    /// An article's comments as written, held against the markdown their hash
    /// names — see [`Comments::carry`].
    fn comments(&self, id: &str) -> Comments;

    /// Write an article's comments, their ranges into its markdown as it
    /// stands. No comments takes the file out.
    fn save_comments(
        &self,
        id: &str,
        comments: &Comments,
    ) -> impl Future<Output = Result<()>> + Send + use<Self>;

    /// What an article holds beside its markdown. Default for one that has
    /// none.
    fn properties(&self, id: &str) -> Properties;

    /// Write the properties back. Keys this crate does not know about are
    /// kept.
    fn save_properties(
        &self,
        id: &str,
        properties: &Properties,
    ) -> impl Future<Output = Result<()>> + Send + use<Self>;

    /// Take an article out, its assets and cover with it, and retire its
    /// number.
    fn remove_article(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<Self>;

    // ── assets ───────────────────────────────────────────────────────

    /// One of `owner`'s pictures, by the name it was filed under.
    fn asset(
        &self,
        owner: &Owner,
        name: &str,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send + use<Self>;

    /// File a picture among `owner`'s, over any of the same name. Refused for
    /// an article or a board that is not here, and for a name that is not
    /// one path component.
    fn put_asset(
        &self,
        owner: &Owner,
        name: &str,
        bytes: &[u8],
    ) -> impl Future<Output = Result<()>> + Send + use<Self>;

    // ── files ────────────────────────────────────────────────────────

    /// Copy a card's files to where it is landing as `to_card` on `to_board`
    /// in `to`, which may be another project. Run before either board is
    /// saved: the source's save removes the card's files. Nothing to copy
    /// answers `Ok`.
    fn carry_card_files(
        &self,
        board: &str,
        card: &str,
        to: &Self,
        to_board: &str,
        to_card: &str,
    ) -> impl Future<Output = Result<()>> + Send + use<Self> {
        let _ = (board, card, to, to_board, to_card);
        std::future::ready(Ok(()))
    }

    /// Where an entry is, as a path on the host this backend keeps its files
    /// on. `None` for one that is not there, for a table, and from a backend
    /// that keeps no files.
    fn place(&self, kind: Kind, id: &str) -> Option<PathBuf> {
        let _ = (kind, id);
        None
    }

    /// Whether `dir`, a path on the host this backend keeps its files on, is
    /// the folder of a document filed here: an article's, a board's
    /// description, or a card's on one of its boards.
    fn files(&self, dir: &Path) -> bool {
        let _ = dir;
        false
    }

    // ── changes ──────────────────────────────────────────────────────

    /// Call `knock` whenever something this backend reads back changes under
    /// it, from any thread. A knock carries nothing: the answer to one is a
    /// re-read. `None` for a backend that has nothing to watch or cannot.
    fn watch(&self, knock: impl Fn() + Send + Sync + 'static) -> Option<Watching> {
        let _ = knock;
        None
    }

    // ── numbers ──────────────────────────────────────────────────────

    /// The project-wide number an entry is referred to by (`#12`), issued on
    /// first ask. Every client of one project must get the same answer.
    fn number(&self, kind: Kind, id: &str) -> impl Future<Output = Result<u64>> + Send + use<Self>;

    /// The id a number was issued to, or `None` for one never issued or
    /// retired.
    fn resolve(&self, kind: Kind, number: u64) -> Result<Option<String>>;

    /// Retire an entry's number. It is never issued again.
    fn retire(&self, kind: Kind, id: &str) -> impl Future<Output = Result<()>> + Send + use<Self>;
}
