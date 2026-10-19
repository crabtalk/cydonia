//! Every backend this build can hold a project in, as one [`Project`].
//!
//! Which backend stands at a [`crate::location::Location`] is the holder's to
//! pick: this names none.

#[cfg(feature = "remote")]
use super::remote;
use super::{Owner, Project, Result, Watching, fs, memory};
#[cfg(feature = "sqlite")]
use crate::table::{Home, Local as _, sqlite::Sqlite};
use crate::{
    article::{Article, comments::Comments, properties::Properties},
    board::Board,
    entry::{self, Entry},
    kind::Kind,
    search::{self, Document, Match, Query, Searchable},
    session::record::Record,
    table::Tables,
};
use futures::{FutureExt as _, executor::block_on, future::BoxFuture};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc::Sender},
};

/// Rows a table's entry reads.
const PREVIEW: i64 = 200;

#[derive(Clone)]
pub enum Store {
    Fs(fs::Project),
    Memory(Arc<memory::Project>),
    #[cfg(feature = "remote")]
    Remote(remote::Project),
}

impl Store {
    /// The project's tables as they already are. `None` while a project on
    /// this machine has none, and from a memory project. Writes nothing.
    pub fn tables(&self) -> Option<Box<dyn Tables>> {
        match self {
            #[cfg(feature = "sqlite")]
            Store::Fs(store) => Sqlite::attach(&Home::Project(store.root().to_owned()))
                .map(|tables| Box::new(tables) as Box<dyn Tables>),
            #[cfg(not(feature = "sqlite"))]
            Store::Fs(_) => None,
            Store::Memory(_) => None,
            #[cfg(feature = "remote")]
            Store::Remote(store) => Some(Box::new(store.tables())),
        }
    }

    /// The project's tables, made if it has none yet.
    pub fn make_tables(&self) -> anyhow::Result<Box<dyn Tables>> {
        match self {
            #[cfg(feature = "sqlite")]
            Store::Fs(store) => Ok(Box::new(Sqlite::open(&Home::Project(
                store.root().to_owned(),
            ))?)),
            #[cfg(not(feature = "sqlite"))]
            Store::Fs(_) => anyhow::bail!("There are no tables in this build"),
            Store::Memory(_) => anyhow::bail!("This project holds no tables"),
            #[cfg(feature = "remote")]
            Store::Remote(store) => Ok(Box::new(store.tables())),
        }
    }

    /// Where `owner`'s picture `name` is, as a path on the host the backend
    /// keeps its files on, whether or not it has been written. `None` from a
    /// backend that keeps no files.
    pub fn asset_file(&self, owner: &Owner, name: &str) -> Option<PathBuf> {
        match self {
            Store::Fs(store) => Some(store.asset_dir(owner).ok()?.join(name)),
            Store::Memory(_) => None,
            #[cfg(feature = "remote")]
            Store::Remote(store) => Some(store.asset_dir(owner).ok()?.join(name)),
        }
    }

    /// Every entry, the [`entry::catalog`] and the tables, by number. Makes
    /// no storage in a project that has none.
    pub fn entries(&self) -> anyhow::Result<Vec<Entry>> {
        let mut entries = entry::catalog(self)?;
        if let Some(tables) = self.tables() {
            for table in block_on(tables.list())? {
                let number = table
                    .number
                    .ok_or_else(|| anyhow::anyhow!("no number for table {}", table.key))?;
                entries.push(Entry {
                    number,
                    kind: Kind::Table,
                    id: table.key,
                    title: table.name,
                    archived: table.archived,
                    labels: Vec::new(),
                    article: None,
                });
            }
        }
        entries.sort_by_key(|entry| entry.number);
        Ok(entries)
    }

    /// One of [`Store::entries`] as a client reads it. A table reads its
    /// first [`PREVIEW`] rows and how many it holds.
    pub fn read(&self, entry: &Entry) -> anyhow::Result<serde_json::Value> {
        if let Some(value) = entry::open(self, entry)? {
            return Ok(value);
        }
        anyhow::ensure!(
            entry.kind == Kind::Table,
            "unknown entry kind {}",
            entry.kind
        );
        let tables = self
            .tables()
            .ok_or_else(|| anyhow::anyhow!("this project holds no tables"))?;
        let page = block_on(tables.read(&entry.id, None, false, PREVIEW, 0))?;
        let columns: Vec<&str> = page
            .columns
            .iter()
            .map(|column| column.name.as_str())
            .collect();
        let rows: Vec<&[serde_json::Value]> = page.rows.iter().map(|row| &row.cells[..]).collect();
        Ok(serde_json::json!({
            "key": page.key,
            "columns": columns,
            "rows": rows,
            "total": page.total,
            "limit": PREVIEW,
        }))
    }

    /// Every match of `query` in the items of `kinds`, sent to `found`. Reads
    /// only.
    pub fn search(&self, kinds: &[Kind], query: &Query, found: &Sender<Match>) {
        if let Store::Fs(store) = self {
            return search::disk(store, kinds, query, found);
        }
        let boards = match kinds.contains(&Kind::Board) {
            true => self.boards(),
            false => Vec::new(),
        };
        let sessions = match kinds.contains(&Kind::Session) {
            true => self.sessions(),
            false => Vec::new(),
        };
        let articles: Vec<(Article, String)> = match kinds.contains(&Kind::Article) {
            true => self
                .articles()
                .into_iter()
                .filter_map(|article| {
                    let markdown = self.read_article(&article.id).ok()?;
                    Some((article, markdown))
                })
                .collect(),
            false => Vec::new(),
        };
        let documents: Vec<Document<'_>> = articles
            .iter()
            .map(|(article, markdown)| Document { article, markdown })
            .collect();
        let sources: Vec<&(dyn Searchable + Sync)> = boards
            .iter()
            .map(|board| board as &(dyn Searchable + Sync))
            .chain(
                sessions
                    .iter()
                    .map(|record| record as &(dyn Searchable + Sync)),
            )
            .chain(
                documents
                    .iter()
                    .map(|document| document as &(dyn Searchable + Sync)),
            )
            .collect();
        search::run(&sources, query, found);
    }
}

/// The same call on whichever backend is held.
macro_rules! each {
    ($self:ident, $store:ident => $call:expr) => {
        match $self {
            Store::Fs($store) => $call,
            Store::Memory($store) => $call,
            #[cfg(feature = "remote")]
            Store::Remote($store) => $call,
        }
    };
}

/// [`each!`] for a call answered with a future, a different one per backend.
macro_rules! each_future {
    ($self:ident, $store:ident => $call:expr) => {{
        let answer: BoxFuture<'static, _> = each!($self, $store => $call.boxed());
        answer
    }};
}

impl Project for Store {
    fn boards(&self) -> Vec<Board> {
        each!(self, store => store.boards())
    }

    fn board(&self, id: &str) -> Option<Board> {
        each!(self, store => store.board(id))
    }

    fn create_board(
        &self,
        name: &str,
        key: &str,
    ) -> impl Future<Output = Result<Board>> + Send + use<> {
        each_future!(self, store => store.create_board(name, key))
    }

    fn save_board(&self, board: &mut Board) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.save_board(board))
    }

    fn remove_board(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.remove_board(id))
    }

    fn sessions(&self) -> Vec<Record> {
        each!(self, store => store.sessions())
    }

    fn session(&self, id: &str) -> Option<Record> {
        each!(self, store => store.session(id))
    }

    fn create_session(&self) -> impl Future<Output = Result<String>> + Send + use<> {
        each_future!(self, store => store.create_session())
    }

    fn save_session(&self, record: &Record) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.save_session(record))
    }

    fn remove_session(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.remove_session(id))
    }

    fn articles(&self) -> Vec<Article> {
        each!(self, store => store.articles())
    }

    fn article(&self, id: &str) -> Option<Article> {
        each!(self, store => store.article(id))
    }

    fn create_article(
        &self,
        markdown: &str,
    ) -> impl Future<Output = Result<Article>> + Send + use<> {
        each_future!(self, store => store.create_article(markdown))
    }

    fn read_article(&self, id: &str) -> Result<String> {
        each!(self, store => store.read_article(id))
    }

    fn write_article(
        &self,
        id: &str,
        markdown: &str,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.write_article(id, markdown))
    }

    fn comments(&self, id: &str) -> Comments {
        each!(self, store => store.comments(id))
    }

    fn save_comments(
        &self,
        id: &str,
        comments: &Comments,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.save_comments(id, comments))
    }

    fn properties(&self, id: &str) -> Properties {
        each!(self, store => store.properties(id))
    }

    fn save_properties(
        &self,
        id: &str,
        properties: &Properties,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.save_properties(id, properties))
    }

    fn remove_article(&self, id: &str) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.remove_article(id))
    }

    fn asset(
        &self,
        owner: &Owner,
        name: &str,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send + use<> {
        each_future!(self, store => store.asset(owner, name))
    }

    fn put_asset(
        &self,
        owner: &Owner,
        name: &str,
        bytes: &[u8],
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.put_asset(owner, name, bytes))
    }

    /// Nothing is carried between two different backends.
    fn carry_card_files(
        &self,
        board: &str,
        card: &str,
        to: &Self,
        to_board: &str,
        to_card: &str,
    ) -> impl Future<Output = Result<()>> + Send + use<> {
        match (self, to) {
            (Store::Fs(from), Store::Fs(to)) => from
                .carry_card_files(board, card, to, to_board, to_card)
                .boxed(),
            (Store::Memory(from), Store::Memory(to)) => from
                .carry_card_files(board, card, to, to_board, to_card)
                .boxed(),
            #[cfg(feature = "remote")]
            (Store::Remote(from), Store::Remote(to)) => from
                .carry_card_files(board, card, to, to_board, to_card)
                .boxed(),
            _ => std::future::ready(Ok(())).boxed(),
        }
    }

    fn place(&self, kind: Kind, id: &str) -> Option<PathBuf> {
        each!(self, store => store.place(kind, id))
    }

    fn files(&self, dir: &Path) -> bool {
        each!(self, store => store.files(dir))
    }

    fn watch(&self, knock: impl Fn() + Send + Sync + 'static) -> Option<Watching> {
        each!(self, store => store.watch(knock))
    }

    fn number(&self, kind: Kind, id: &str) -> impl Future<Output = Result<u64>> + Send + use<> {
        each_future!(self, store => store.number(kind, id))
    }

    fn resolve(&self, kind: Kind, number: u64) -> Result<Option<String>> {
        each!(self, store => store.resolve(kind, number))
    }

    fn retire(&self, kind: Kind, id: &str) -> impl Future<Output = Result<()>> + Send + use<> {
        each_future!(self, store => store.retire(kind, id))
    }
}
