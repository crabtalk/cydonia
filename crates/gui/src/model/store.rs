//! The backend a project's work is kept in.
//!
//! Every read and write of boards, sessions, articles, numbers and tables goes
//! through the [`Store`] that [`open`] answers, and `open` is the one place
//! that picks the backend: the memory project [`seed`] put at a location, else
//! the files at a location on this machine, else the project
//! [`crate::model::remote::Remotes`] has open at a location on another
//! machine. A memory project holds no tables.

use crate::data::{Data, Here, Home, Local as _};
use artifact::project::Result;
#[cfg(feature = "desktop")]
use artifact::project::remote;
use artifact::{
    article::{Article, comments::Comments, properties::Properties},
    board::Board,
    kind::Kind,
    location::Location,
    project::{Owner, Project, Watching, fs, memory},
    session::record::Record,
};
use futures::{FutureExt as _, future::BoxFuture};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

/// Every backend this build can hold a project in.
#[derive(Clone)]
pub enum Store {
    Fs(fs::Project),
    Memory(Arc<memory::Project>),
    #[cfg(feature = "desktop")]
    Remote(remote::Project),
}

/// Projects held in memory, by the location they stand at, for the life of
/// the process.
fn seeded() -> &'static Mutex<HashMap<Location, Arc<memory::Project>>> {
    static SEEDED: OnceLock<Mutex<HashMap<Location, Arc<memory::Project>>>> = OnceLock::new();
    SEEDED.get_or_init(Default::default)
}

/// Stand `project` at `location`: every [`open`] of that location answers it
/// from here on, and nothing is read from or written to the disk there.
pub fn seed(location: impl Into<Location>, project: memory::Project) {
    seeded()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(location.into(), Arc::new(project));
}

/// `None` for a location on another machine that no project is open at.
pub fn open(location: &Location) -> Option<Store> {
    let held = seeded()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(location)
        .cloned();
    match (held, location) {
        (Some(project), _) => Some(Store::Memory(project)),
        (None, Location::Local(root)) => Some(Store::Fs(fs::Project::new(root))),
        #[cfg(feature = "desktop")]
        (None, Location::Remote { .. }) => crate::model::remote::Remotes::get()
            .ok()?
            .project(location)
            .map(Store::Remote),
    }
}

impl Store {
    /// The project's tables as they already are. `None` while a project on
    /// this machine has none, and from a memory project. Writes nothing.
    pub fn tables(&self) -> Option<Data> {
        match self {
            Store::Fs(store) => Here::attach(&Home::Project(store.root().to_owned()))
                .map(|tables| Box::new(tables) as Data),
            Store::Memory(_) => None,
            #[cfg(feature = "desktop")]
            Store::Remote(store) => Some(Box::new(store.tables())),
        }
    }

    /// Where `owner`'s picture `name` is, as a path on the host the backend
    /// keeps its files on, whether or not it has been written. `None` from a
    /// backend that keeps no files.
    pub fn asset_file(&self, owner: &Owner, name: &str) -> Option<PathBuf> {
        match self {
            Store::Fs(store) => Some(store.asset_dir(owner).ok()?.join(name)),
            Store::Memory(_) => None,
            #[cfg(feature = "desktop")]
            Store::Remote(store) => Some(store.asset_dir(owner).ok()?.join(name)),
        }
    }

    /// The project's tables, made if it has none yet.
    pub fn make_tables(&self) -> anyhow::Result<Data> {
        match self {
            Store::Fs(store) => Ok(Box::new(Here::open(&Home::Project(
                store.root().to_owned(),
            ))?)),
            Store::Memory(_) => anyhow::bail!("This project holds no tables"),
            #[cfg(feature = "desktop")]
            Store::Remote(store) => Ok(Box::new(store.tables())),
        }
    }
}

/// The same call on whichever backend is held.
macro_rules! each {
    ($self:ident, $store:ident => $call:expr) => {
        match $self {
            Store::Fs($store) => $call,
            Store::Memory($store) => $call,
            #[cfg(feature = "desktop")]
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
            #[cfg(feature = "desktop")]
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
