//! The backend a project's work is kept in.
//!
//! Every read and write of boards, sessions, articles and numbers goes through
//! the [`Store`] that [`open`] answers, and `open` is the one place that picks
//! the backend.

use anyhow::Result;
use artifact::{
    article::{Article, properties::Properties},
    board::Board,
    project::{Project, Watching, fs},
    session::record::Record,
};
use std::path::Path;

/// Every backend this build can hold a project in.
#[derive(Clone)]
pub enum Store {
    Fs(fs::Project),
}

pub fn open(path: &Path) -> Store {
    Store::Fs(fs::Project::new(path))
}

/// The same call on whichever backend is held.
macro_rules! each {
    ($self:ident, $store:ident => $call:expr) => {
        match $self {
            Store::Fs($store) => $call,
        }
    };
}

impl Project for Store {
    fn boards(&self) -> Vec<Board> {
        each!(self, store => store.boards())
    }

    fn board(&self, id: &str) -> Option<Board> {
        each!(self, store => store.board(id))
    }

    fn create_board(&self, name: &str, key: &str) -> Result<Board> {
        each!(self, store => store.create_board(name, key))
    }

    fn save_board(&self, board: &mut Board) -> Result<()> {
        each!(self, store => store.save_board(board))
    }

    fn remove_board(&self, id: &str) -> Result<()> {
        each!(self, store => store.remove_board(id))
    }

    fn sessions(&self) -> Vec<Record> {
        each!(self, store => store.sessions())
    }

    fn session(&self, id: &str) -> Option<Record> {
        each!(self, store => store.session(id))
    }

    fn create_session(&self) -> Result<String> {
        each!(self, store => store.create_session())
    }

    fn save_session(&self, record: &Record) -> Result<()> {
        each!(self, store => store.save_session(record))
    }

    fn remove_session(&self, id: &str) -> Result<()> {
        each!(self, store => store.remove_session(id))
    }

    fn articles(&self) -> Vec<Article> {
        each!(self, store => store.articles())
    }

    fn article(&self, id: &str) -> Option<Article> {
        each!(self, store => store.article(id))
    }

    fn create_article(&self, markdown: &str) -> Result<Article> {
        each!(self, store => store.create_article(markdown))
    }

    fn read_article(&self, id: &str) -> Result<String> {
        each!(self, store => store.read_article(id))
    }

    fn write_article(&self, id: &str, markdown: &str) -> Result<()> {
        each!(self, store => store.write_article(id, markdown))
    }

    fn properties(&self, id: &str) -> Properties {
        each!(self, store => store.properties(id))
    }

    fn save_properties(&self, id: &str, properties: &Properties) -> Result<()> {
        each!(self, store => store.save_properties(id, properties))
    }

    fn remove_article(&self, id: &str) -> Result<()> {
        each!(self, store => store.remove_article(id))
    }

    fn asset(&self, id: &str, name: &str) -> Result<Vec<u8>> {
        each!(self, store => store.asset(id, name))
    }

    fn put_asset(&self, id: &str, name: &str, bytes: &[u8]) -> Result<()> {
        each!(self, store => store.put_asset(id, name, bytes))
    }

    fn watch(&self, knock: impl Fn() + Send + Sync + 'static) -> Option<Watching> {
        each!(self, store => store.watch(knock))
    }

    fn number(&self, kind: &str, id: &str) -> Result<u64> {
        each!(self, store => store.number(kind, id))
    }

    fn resolve(&self, kind: &str, number: u64) -> Result<Option<String>> {
        each!(self, store => store.resolve(kind, number))
    }

    fn retire(&self, kind: &str, id: &str) -> Result<()> {
        each!(self, store => store.retire(kind, id))
    }
}
