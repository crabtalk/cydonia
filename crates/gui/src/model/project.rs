//! A project: a directory, the sessions running in it, its boards and its
//! articles.
//!
//! Its [`Location`] is the whole identity — its root is what every session in
//! the project is spawned with as its `cwd`, and it is what
//! [`crate::model::state`] persists.

use crate::{
    data::{Data, Page, Table},
    model::{
        article::{self, Article},
        session::ChatSession,
        store::Store,
        watch::Watch,
        workspace::{Showing, Workspace},
    },
};
use artifact::board::Board;
use artifact::kind::Kind;
use artifact::location::Location;
use artifact::project::Project as _;
use bezel::gpui::{App, AppContext as _, Context, Entity, Task};
use futures::future;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// How many rows the table pane reads at once. The count beside them is the
/// table's own, so a window that does not reach the end says so.
const PAGE: i64 = 200;

/// What every entry has, whatever its kind. `touched` is in milliseconds.
pub struct Facts {
    pub showing: Showing,
    pub label: String,
    pub number: Option<u64>,
    pub touched: u128,
    pub created: Option<u128>,
    pub archived: bool,
    /// Empty for a table, which carries none.
    pub labels: Vec<String>,
    /// The agent a session runs on; `None` for every other kind.
    pub agent: Option<String>,
}

/// A board a project holds: its id, which never changes, and the board, which
/// every view drawing any of it observes.
#[derive(Clone)]
pub struct HeldBoard {
    pub id: String,
    pub board: Entity<Board>,
}

impl HeldBoard {
    pub fn new(board: Board, cx: &mut App) -> Self {
        Self {
            id: board.id.clone(),
            board: cx.new(|_| board),
        }
    }
}

/// A session a project holds: its id, which never changes, and the session,
/// which every view drawing any of it observes.
#[derive(Clone)]
pub struct HeldSession {
    pub id: u64,
    pub chat: Entity<ChatSession>,
}

impl HeldSession {
    pub fn new(chat: ChatSession, cx: &mut App) -> Self {
        Self {
            id: chat.id,
            chat: cx.new(|_| chat),
        }
    }
}

pub struct Project {
    location: Location,
    /// Where this project's work is kept — see [`crate::model::store::open`].
    store: Store,
    pub sessions: Vec<HeldSession>,
    pub active: Option<u64>,
    pub boards: Vec<HeldBoard>,
    /// Which board the board pane shows.
    pub board: Option<usize>,
    pub articles: Vec<Article>,
    /// Which article the article pane shows.
    pub article: Option<usize>,
    /// The project's database, once there is one. Opening a project must not
    /// write a database into it, so this stays `None` until a table is made.
    pub data: Option<Data>,
    pub tables: Vec<Table>,
    /// Which table the table pane shows.
    pub table: Option<usize>,
    /// The window of rows each table on screen was last read with, by the
    /// table's key. Read when a table is opened rather than while it is drawn
    /// — a query per frame is a query too many.
    ///
    /// By key rather than one slot: a space can stand two tables side by
    /// side, and one page between them would draw the same rows in both.
    ///
    /// A page stays as it is until a newer read of it lands.
    pub pages: HashMap<String, Page>,
    /// Reads issued on `data`, which numbers each one.
    asked: u64,
    /// The read `tables` was last taken from.
    listed: u64,
    /// The read each page in `pages` was taken from, by key.
    paged: HashMap<String, u64>,
    /// Tables drawn somewhere other than the table pane — a drawer — by key.
    /// Their pages are read alongside the open table's.
    held: std::collections::BTreeSet<String>,
    /// Whether the sidebar shows what is under this project's heading.
    pub expanded: bool,
    /// Whether it shows what is under the archived divider. Folded away by
    /// default: what was put away is not what you came back for.
    pub archive_open: bool,
    /// The one label the sidebar lists this project's entries by, or every
    /// entry. Runtime only: a relaunch lists them all.
    pub label: Option<String>,
    /// The watch on this project's `.cydonia/`, once it is up. Held here so
    /// closing the project drops it, which is what takes the watch down.
    pub watch: Option<Watch>,
}

impl Project {
    /// The entry of `kind` this project has selected.
    pub fn selected(&self, kind: Kind) -> Option<Showing> {
        Some(match kind {
            Kind::Session => Showing::Session(self.active?),
            Kind::Board => Showing::Board(self.boards.get(self.board?)?.id.clone()),
            Kind::Article => Showing::Article(self.articles.get(self.article?)?.id.clone()),
            Kind::Table => Showing::Table(self.tables.get(self.table?)?.key.clone()),
            Kind::App => return None,
        })
    }

    /// The project at `location`, kept where [`crate::model::store::open`]
    /// finds it, its tables not yet read. `None` where it finds none.
    pub fn at(location: Location, cx: &mut App) -> Option<Self> {
        let store = crate::model::store::open(&location)?;
        Some(Self::new(location, store, cx))
    }

    /// The project at `location`, its work kept in `store`, its tables not yet
    /// read — see [`Self::reload_tables`].
    pub fn new(location: Location, store: Store, cx: &mut App) -> Self {
        Self {
            boards: store
                .boards()
                .into_iter()
                .map(|board| HeldBoard::new(board, cx))
                .collect(),
            articles: article::list(&location, &store),
            data: store.tables(),
            location,
            store,
            sessions: Vec::new(),
            active: None,
            board: None,
            article: None,
            tables: Vec::new(),
            table: None,
            pages: HashMap::new(),
            asked: 0,
            listed: 0,
            paged: HashMap::new(),
            held: Default::default(),
            expanded: true,
            archive_open: false,
            label: None,
            watch: None,
        }
    }

    /// The board whose key is `key`, matched as a handle matches it.
    pub fn keyed_board(&self, key: &str, cx: &App) -> Option<&Entity<Board>> {
        self.boards
            .iter()
            .map(|held| &held.board)
            .find(|board| board.read(cx).keyed(key))
    }

    /// The board listed at `ix`.
    pub fn board<'a>(&self, ix: usize, cx: &'a App) -> Option<&'a Board> {
        Some(self.boards.get(ix)?.board.read(cx))
    }

    /// Re-read everything on disk and reconcile it with what is held. The
    /// answer to any event under `.cydonia/` — see [`crate::model::watch`] for
    /// why the event itself is never read for more than where it landed.
    ///
    /// Answers whether anything a view holds *about* an entry moved: a card's
    /// place on a board. Everything else is drawn off the model each frame and
    /// nobody keeps a handle on it, so a re-read that only changed those has
    /// nothing to announce — and announcing it would drop the edit somebody has
    /// open over the echo of their own save. The tables are re-read by
    /// [`Self::reload_tables`].
    pub fn reload(&mut self, cx: &mut Context<Workspace>) -> bool {
        // The store can appear long after the project was opened, and an agent
        // making one is exactly the case this watch is here for.
        if self.data.is_none() {
            self.data = self.store().tables();
        }
        self.reload_articles(cx);
        self.reload_boards(cx)
    }

    /// What the table pane addresses by position: which tables there are, and
    /// what the open one's columns are called.
    fn shape(&self) -> (Vec<String>, Vec<String>) {
        let keys = self.tables.iter().map(|table| table.key.clone()).collect();
        let columns = self
            .open_page()
            .map(|page| page.columns.iter().map(|col| col.name.clone()).collect())
            .unwrap_or_default();
        (keys, columns)
    }

    /// Re-list the articles and merge the re-read into what is open.
    ///
    /// Held by path across the merge, not by index, and for the reason
    /// [`Self::reload_tables`] holds by key: the list is ordered by when each
    /// was last written, so one article changing can move every other one.
    fn reload_articles(&mut self, cx: &mut Context<Workspace>) {
        let open = self.open_article().map(Path::to_path_buf);
        let mut held: HashMap<PathBuf, Article> = self
            .articles
            .drain(..)
            .map(|article| (article.path.clone(), article))
            .collect();
        self.articles = article::list(&self.location, &self.store)
            .into_iter()
            .map(|fresh| match held.remove(&fresh.path) {
                // Already on screen — the document under it and its undo
                // history stay where they are.
                Some(mut article) => {
                    article.adopt(&fresh, cx);
                    article
                }
                None => fresh,
            })
            .collect();
        self.article = open.and_then(|path| self.articles.iter().position(|at| at.path == path));
    }

    /// The file the article pane is showing, if it is showing one.
    fn open_article(&self) -> Option<&Path> {
        let article = self.articles.get(self.article?)?;
        Some(&article.path)
    }

    /// The same, for boards. Nothing here is a live surface, so the merge is
    /// only about not disturbing a board the re-read did not change — see
    /// [`Board::adopt`], whose answer is carried out of here.
    fn reload_boards(&mut self, cx: &mut App) -> bool {
        let open = self
            .board
            .and_then(|ix| self.boards.get(ix))
            .map(|held| held.id.clone());
        let mut held: HashMap<String, HeldBoard> = self
            .boards
            .drain(..)
            .map(|held| (held.id.clone(), held))
            .collect();
        let mut moved = false;
        let fresh = self.store().boards();
        self.boards = fresh
            .into_iter()
            .map(|fresh| match held.remove(&fresh.id) {
                // The entity stays, so every view drawing the board keeps
                // drawing it: what changes is what it holds.
                Some(kept) => {
                    kept.board.update(cx, |board, cx| {
                        moved |= board.adopt(fresh);
                        cx.notify();
                    });
                    kept
                }
                None => HeldBoard::new(fresh, cx),
            })
            .collect();
        self.board = open.and_then(|id| self.boards.iter().position(|at| at.id == id));
        moved
    }

    /// Re-read what tables exist, and the pages on show. The store is the
    /// list — nothing here keeps a second copy of it that a failed write could
    /// leave standing. Answers, once both have landed, whether what the table
    /// pane addresses by position moved — see [`Self::shape`].
    pub fn reload_tables(&mut self, cx: &mut Context<Workspace>) -> Task<bool> {
        let seq = self.ask();
        let listed = self.data.as_ref().map(|data| data.list());
        let pages = self.read_pages();
        let location = self.location.clone();
        cx.spawn(async move |workspace, cx| {
            let tables = match listed {
                Some(listed) => listed.await.unwrap_or_default(),
                None => Vec::new(),
            };
            let pages = pages.await;
            workspace
                .update(cx, |workspace, cx| {
                    let ix = workspace.project_at(&location)?;
                    let open = &mut workspace.projects[ix];
                    let before = open.shape();
                    open.take_tables(seq, tables);
                    open.take_pages(seq, pages);
                    cx.notify();
                    Some(before != open.shape())
                })
                .ok()
                .flatten()
                .unwrap_or(false)
        })
    }

    /// Whether a list of the tables has landed since the project was opened.
    pub fn tables_listed(&self) -> bool {
        self.listed > 0
    }

    /// Take a list read with `seq`, unless a newer one has landed.
    fn take_tables(&mut self, seq: u64, tables: Vec<Table>) {
        if seq <= self.listed {
            return;
        }
        self.listed = seq;
        // Held by key across the re-read, not by index: a table made or dropped
        // beside the open one shifts every index past it, and the pane would be
        // left showing whichever table slid into its place.
        let open = self
            .table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone());
        self.tables = tables;
        self.table = match open.and_then(|key| self.table_ix(&key)) {
            found @ Some(_) => found,
            None => self.table.filter(|ix| *ix < self.tables.len()),
        };
    }

    /// The page the open table is on.
    pub fn open_page(&self) -> Option<&Page> {
        let key = &self.tables.get(self.table?)?.key;
        self.pages.get(key)
    }

    /// Read the rows for the open table and the held ones.
    pub fn reload_page(&mut self, cx: &mut Context<Workspace>) {
        let seq = self.ask();
        let pages = self.read_pages();
        let location = self.location.clone();
        cx.spawn(async move |workspace, cx| {
            let pages = pages.await;
            let _ = workspace.update(cx, |workspace, cx| {
                if let Some(ix) = workspace.project_at(&location) {
                    workspace.projects[ix].take_pages(seq, pages);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The tables whose pages are read: the open one and the held ones.
    fn wanted_pages(&self) -> Vec<String> {
        self.table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone())
            .into_iter()
            .chain(self.held.iter().cloned())
            .collect()
    }

    /// Issue the reads of the wanted pages. Any other page is dropped: a page
    /// is a window on a table nobody is looking at.
    fn read_pages(&mut self) -> impl Future<Output = Vec<(String, anyhow::Result<Page>)>> + use<> {
        let wanted = self.wanted_pages();
        self.pages.retain(|key, _| wanted.contains(key));
        self.paged.retain(|key, _| wanted.contains(key));
        let reads: Vec<_> = match self.data.as_ref() {
            Some(data) => wanted
                .into_iter()
                .map(|key| {
                    let read = data.read(&key, None, false, PAGE, 0);
                    async move { (key, read.await) }
                })
                .collect(),
            None => {
                self.pages.clear();
                Vec::new()
            }
        };
        future::join_all(reads)
    }

    /// Take pages read with `seq`: each one still wanted, unless a newer read
    /// of it has landed.
    fn take_pages(&mut self, seq: u64, landed: Vec<(String, anyhow::Result<Page>)>) {
        let wanted = self.wanted_pages();
        for (key, page) in landed {
            let Ok(page) = page else {
                continue;
            };
            if wanted.contains(&key) && self.paged.get(&key).is_none_or(|at| *at < seq) {
                self.paged.insert(key.clone(), seq);
                self.pages.insert(key, page);
            }
        }
    }

    /// Number the next read.
    fn ask(&mut self) -> u64 {
        self.asked += 1;
        self.asked
    }

    /// Hold the pages of these tables beside the open one's, read now when the
    /// set changes.
    pub fn hold_pages(
        &mut self,
        held: std::collections::BTreeSet<String>,
        cx: &mut Context<Workspace>,
    ) {
        if self.held != held {
            self.held = held;
            self.reload_page(cx);
        }
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    /// The project's directory, on whichever machine holds it.
    pub fn root(&self) -> &Path {
        self.location.root()
    }

    /// Where this project's work is kept — see [`crate::model::store::open`].
    pub fn store(&self) -> Store {
        self.store.clone()
    }

    /// The tab's label: the directory's own name, or the whole path when it
    /// has none (`/`).
    pub fn name(&self) -> String {
        let root = self.root();
        root.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string())
    }

    pub fn session<'a>(&self, id: u64, cx: &'a App) -> Option<&'a ChatSession> {
        Some(self.session_entity(id)?.read(cx))
    }

    pub fn session_entity(&self, id: u64) -> Option<&Entity<ChatSession>> {
        self.sessions
            .iter()
            .find(|held| held.id == id)
            .map(|held| &held.chat)
    }

    /// The sessions, read.
    pub fn chats<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a ChatSession> + 'a {
        self.sessions.iter().map(|held| held.chat.read(cx))
    }

    /// The sessions not archived, most recently touched first, without the
    /// articles' threads.
    pub fn recent_sessions<'a>(&'a self, cx: &'a App) -> Vec<&'a ChatSession> {
        let mut chats: Vec<&ChatSession> = self
            .chats(cx)
            .filter(|chat| !chat.closed && chat.article.is_none())
            .collect();
        chats.sort_by_key(|chat| std::cmp::Reverse(chat.touched()));
        chats
    }

    /// Where the entry `showing` names is listed among its kind.
    /// The entry a stored id names: a session by its record, every other kind
    /// by its id or key.
    pub fn stored(&self, kind: Kind, id: &str, cx: &App) -> Option<Showing> {
        Some(match kind {
            Kind::Session => Showing::Session(
                self.chats(cx)
                    .find(|chat| chat.record.as_deref() == Some(id))?
                    .id,
            ),
            Kind::Board => Showing::Board(self.boards[self.board_ix(id)?].id.clone()),
            Kind::Article => Showing::Article(self.articles[self.article_ix(id)?].id.clone()),
            Kind::Table => Showing::Table(self.tables[self.table_ix(id)?].key.clone()),
            Kind::App => return None,
        })
    }

    /// What the entry `showing` names carries, whatever its kind.
    pub fn facts(&self, showing: &Showing, cx: &App) -> Option<Facts> {
        Some(self.facts_at(showing.kind(), self.ix_of(showing)?, cx))
    }

    /// Every entry of `kind`, in list order.
    pub fn every<'a>(&'a self, kind: Kind, cx: &'a App) -> impl Iterator<Item = Facts> + 'a {
        let len = match kind {
            Kind::Session => self.sessions.len(),
            Kind::Board => self.boards.len(),
            Kind::Article => self.articles.len(),
            Kind::Table => self.tables.len(),
            Kind::App => 0,
        };
        (0..len).map(move |ix| self.facts_at(kind, ix, cx))
    }

    /// The facts of the `ix`th entry of `kind`. Panics past the end.
    pub fn facts_at(&self, kind: Kind, ix: usize, cx: &App) -> Facts {
        match kind {
            Kind::Session => {
                let chat = self.sessions[ix].chat.read(cx);
                Facts {
                    showing: Showing::Session(chat.id),
                    label: chat.label(),
                    number: chat.number,
                    touched: chat.touched(),
                    created: None,
                    archived: chat.closed,
                    labels: chat.labels.clone(),
                    agent: Some(chat.entry.name.clone()),
                }
            }
            Kind::Board => {
                let board = self.boards[ix].board.read(cx);
                Facts {
                    showing: Showing::Board(board.id.clone()),
                    label: board.label().to_owned(),
                    number: board.number,
                    touched: board.touched,
                    created: board.created,
                    archived: board.archived,
                    labels: board.labels.clone(),
                    agent: None,
                }
            }
            Kind::Article => {
                let article = &self.articles[ix];
                Facts {
                    showing: Showing::Article(article.id.clone()),
                    label: article.label().to_owned(),
                    number: article.number,
                    touched: article.touched,
                    created: article.created,
                    archived: article.archived,
                    labels: article.labels.clone(),
                    agent: None,
                }
            }
            Kind::Table => {
                let table = &self.tables[ix];
                Facts {
                    showing: Showing::Table(table.key.clone()),
                    label: table.name.clone(),
                    number: table.number,
                    touched: table.touched_ms(),
                    created: Some(table.created_ms()),
                    archived: table.archived,
                    labels: Vec::new(),
                    agent: None,
                }
            }
            Kind::App => unreachable!("an app is no project's entry"),
        }
    }

    /// The start of the `ix`th entry of `kind` as plain text: an article's
    /// body, a session's last response from the agent, a board's description
    /// or, for a board without one, its newest card. Empty for a table.
    /// Panics past the end.
    pub fn preview_at(&self, kind: Kind, ix: usize, cx: &App) -> String {
        match kind {
            Kind::Session => self.sessions[ix].chat.read(cx).preview(),
            Kind::Board => {
                let board = self.boards[ix].board.read(cx);
                match board.description.is_empty() {
                    false => artifact::text::preview(&board.description),
                    true => board
                        .newest_card()
                        .map(|card| artifact::text::preview(&card.text))
                        .unwrap_or_default(),
                }
            }
            Kind::Article => self.articles[ix].preview().to_owned(),
            Kind::Table => String::new(),
            Kind::App => unreachable!("an app is no project's entry"),
        }
    }

    pub fn ix_of(&self, showing: &Showing) -> Option<usize> {
        match showing {
            Showing::Session(id) => self.sessions.iter().position(|held| held.id == *id),
            Showing::Board(id) => self.board_ix(id),
            Showing::Article(id) => self.article_ix(id),
            Showing::Table(key) => self.table_ix(key),
        }
    }

    /// Where the board with `id` is listed.
    pub fn board_ix(&self, id: &str) -> Option<usize> {
        self.boards.iter().position(|board| board.id == id)
    }

    /// Where the article with `id` is listed.
    pub fn article_ix(&self, id: &str) -> Option<usize> {
        self.articles.iter().position(|article| article.id == id)
    }

    /// Where the table with `key` is listed.
    pub fn table_ix(&self, key: &str) -> Option<usize> {
        self.tables.iter().position(|table| table.key == key)
    }

    pub fn active_session<'a>(&self, cx: &'a App) -> Option<&'a ChatSession> {
        self.active.and_then(|id| self.session(id, cx))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/archive_boards.rs"]
mod archive_tests;
