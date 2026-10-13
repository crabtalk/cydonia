//! A project: a directory, the sessions running in it, its boards and its
//! articles.
//!
//! The path is the whole identity — it is what every session in the project
//! is spawned with as its `cwd`, and what [`crate::model::state`] persists.

use crate::{
    data::{Data, Page, Table},
    model::{
        article::{self, Article},
        session::ChatSession,
        store::{self, Store},
        watch::Watch,
        workspace::{Showing, Workspace},
    },
};
use artifact::board::Board;
use artifact::kind::Kind;
use artifact::project::Project as _;
use bezel::gpui::{App, AppContext as _, Context, Entity};
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
    pub path: PathBuf,
    pub sessions: Vec<HeldSession>,
    pub active: Option<u64>,
    pub boards: Vec<HeldBoard>,
    unloaded_boards: std::collections::HashSet<String>,
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
    pub pages: HashMap<String, Page>,
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
        })
    }

    pub fn new(path: PathBuf, cx: &mut App) -> Self {
        let mut this = Self {
            boards: store::open(&path)
                .boards()
                .into_iter()
                .map(|board| HeldBoard::new(board, cx))
                .collect(),
            unloaded_boards: Default::default(),
            articles: article::list(&path),
            data: Data::attach(&path),
            path,
            sessions: Vec::new(),
            active: None,
            board: None,
            article: None,
            tables: Vec::new(),
            table: None,
            pages: HashMap::new(),
            held: Default::default(),
            expanded: true,
            archive_open: false,
            label: None,
            watch: None,
        };
        this.reload_tables();
        this.unload_boards(None, cx);
        this
    }

    pub fn load_board(&mut self, id: &str, cx: &mut App) -> bool {
        if !self.unloaded_boards.contains(id) {
            return true;
        }
        let Some(fresh) = self.store().board(id) else {
            return false;
        };
        let Some(held) = self.boards.iter().find(|held| held.id == id) else {
            return false;
        };
        held.board.update(cx, |board, cx| {
            *board = fresh;
            cx.notify();
        });
        self.unloaded_boards.remove(id);
        true
    }

    pub fn unload_boards(&mut self, visible: Option<usize>, cx: &mut App) {
        let store = self.store();
        for (ix, held) in self.boards.iter().enumerate() {
            let board = held.board.read(cx);
            if !board.archived || visible == Some(ix) || self.unloaded_boards.contains(&held.id) {
                continue;
            }
            let Some(saved) = store.board(&held.id) else {
                continue;
            };
            if serde_json::to_value(&saved).ok() != serde_json::to_value(board).ok() {
                continue;
            }
            held.board.update(cx, |board, _| board.columns = Vec::new());
            self.unloaded_boards.insert(held.id.clone());
        }
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
    /// place on a board, a column's place in a table. Everything else is drawn off the model each frame and
    /// nobody keeps a handle on it, so a re-read that only changed those has
    /// nothing to announce — and announcing it would drop the edit somebody has
    /// open over the echo of their own save.
    pub fn reload(&mut self, cx: &mut Context<Workspace>) -> bool {
        // The store can appear long after the project was opened, and an agent
        // making one is exactly the case this watch is here for.
        if self.data.is_none() {
            self.data = Data::attach(&self.path);
        }
        self.reload_articles(cx);
        let boards = self.reload_boards(cx);
        let before = self.shape();
        self.reload_tables();
        boards || before != self.shape()
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
        self.articles = article::list(&self.path)
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
        self.unloaded_boards.clear();
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
        self.unload_boards(self.board, cx);
        moved
    }

    /// Re-read what tables exist. The store is the list — nothing here keeps a
    /// second copy of it that a failed write could leave standing.
    pub fn reload_tables(&mut self) {
        // Held by key across the re-read, not by index: a table made or dropped
        // beside the open one shifts every index past it, and the pane would be
        // left showing whichever table slid into its place.
        let open = self
            .table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone());
        self.tables = self
            .data
            .as_ref()
            .and_then(|data| data.list().ok())
            .unwrap_or_default();
        self.table = match open.and_then(|key| self.table_ix(&key)) {
            found @ Some(_) => found,
            None => self.table.filter(|ix| *ix < self.tables.len()),
        };
        self.reload_page();
    }

    /// Read the open table's rows.
    /// The page the open table is on.
    pub fn open_page(&self) -> Option<&Page> {
        let key = &self.tables.get(self.table?)?.key;
        self.pages.get(key)
    }

    /// Read the rows for the open table and the held ones. Any other page is
    /// dropped: a page is a window on a table nobody is looking at.
    pub fn reload_page(&mut self) {
        let wanted: Vec<String> = self
            .table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone())
            .into_iter()
            .chain(self.held.iter().cloned())
            .collect();
        self.pages.retain(|key, _| wanted.contains(key));
        let Some(data) = self.data.as_ref() else {
            self.pages.clear();
            return;
        };
        for key in wanted {
            if let Ok(page) = data.read(&key, None, false, PAGE, 0) {
                self.pages.insert(key, page);
            }
        }
    }

    /// Hold the pages of these tables beside the open one's, read now when the
    /// set changes.
    pub fn hold_pages(&mut self, held: std::collections::BTreeSet<String>) {
        if self.held != held {
            self.held = held;
            self.reload_page();
        }
    }

    /// Where this project's work is kept — see [`store::open`].
    pub fn store(&self) -> Store {
        store::open(&self.path)
    }

    /// The tab's label: the directory's own name, or the whole path when it
    /// has none (`/`).
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
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
