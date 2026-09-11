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
        watch::Watch,
        workspace::Workspace,
    },
};
use bezel::gpui::Context;
use schema::{backend, board::Board};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// How many rows the table pane reads at once. The count beside them is the
/// table's own, so a window that does not reach the end says so.
const PAGE: i64 = 200;

pub struct Project {
    pub path: PathBuf,
    pub sessions: Vec<ChatSession>,
    pub active: Option<u64>,
    pub boards: Vec<Board>,
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
    /// The open table's window of rows, read when it is opened rather than
    /// while it is drawn — a query per frame is a query too many.
    pub page: Option<Page>,
    /// Whether the sidebar shows what is under this project's heading.
    pub expanded: bool,
    /// Whether it shows what is under the archived divider. Folded away by
    /// default: what was put away is not what you came back for.
    pub archive_open: bool,
    /// The watch on this project's `.cydonia/`, once it is up. Held here so
    /// closing the project drops it, which is what takes the watch down.
    pub watch: Option<Watch>,
}

impl Project {
    pub fn new(path: PathBuf) -> Self {
        let mut this = Self {
            boards: backend::fs::Project::new(&path).boards(),
            articles: article::list(&path),
            data: Data::attach(&path),
            path,
            sessions: Vec::new(),
            active: None,
            board: None,
            article: None,
            tables: Vec::new(),
            table: None,
            page: None,
            expanded: true,
            archive_open: false,
            watch: None,
        };
        this.reload_tables();
        this
    }

    /// Re-read everything on disk and reconcile it with what is held. The
    /// answer to any event under `.cydonia/` — see [`crate::model::watch`] for
    /// why the event itself is never read for more than where it landed.
    ///
    /// Answers whether anything a view holds *about* an entry moved: a card's
    /// place on a board, a column's place in a table, the editor a document was
    /// being read in. Everything else is drawn off the model each frame and
    /// nobody keeps a handle on it, so a re-read that only changed those has
    /// nothing to announce — and announcing it would drop the edit somebody has
    /// open over the echo of their own save.
    pub fn reload(&mut self, cx: &mut Context<Workspace>) -> bool {
        // The store can appear long after the project was opened, and an agent
        // making one is exactly the case this watch is here for.
        if self.data.is_none() {
            self.data = Data::attach(&self.path);
        }
        let articles = self.reload_articles(cx);
        let boards = self.reload_boards();
        let before = self.shape();
        self.reload_tables();
        articles || boards || before != self.shape()
    }

    /// What the table pane addresses by position: which tables there are, and
    /// what the open one's columns are called.
    fn shape(&self) -> (Vec<String>, Vec<String>) {
        let keys = self.tables.iter().map(|table| table.key.clone()).collect();
        let columns = self
            .page
            .as_ref()
            .map(|page| page.columns.iter().map(|col| col.name.clone()).collect())
            .unwrap_or_default();
        (keys, columns)
    }

    /// Re-list the articles and merge the re-read into what is open.
    ///
    /// Held by path across the merge, not by index, and for the reason
    /// [`Self::reload_tables`] holds by key: the list is ordered by when each
    /// was last written, so one article changing can move every other one.
    ///
    /// Carries out [`Article::adopt`]'s answer — an open document rebuilt over
    /// what the file now says has taken the caret with it.
    fn reload_articles(&mut self, cx: &mut Context<Workspace>) -> bool {
        let open = self.open_article().map(Path::to_path_buf);
        let mut held: HashMap<PathBuf, Article> = self
            .articles
            .drain(..)
            .map(|article| (article.path.clone(), article))
            .collect();
        let mut rebuilt = false;
        self.articles = article::list(&self.path)
            .into_iter()
            .map(|fresh| match held.remove(&fresh.path) {
                // Already on screen — the editor over it, its scroll and its
                // undo history stay where they are.
                Some(mut article) => {
                    rebuilt |= article.adopt(&fresh, cx);
                    article
                }
                None => fresh,
            })
            .collect();
        self.article = open.and_then(|path| self.articles.iter().position(|at| at.path == path));
        rebuilt
    }

    /// The file the article pane is showing, if it is showing one.
    fn open_article(&self) -> Option<&Path> {
        let article = self.articles.get(self.article?)?;
        Some(&article.path)
    }

    /// The same, for boards. Nothing here is a live surface, so the merge is
    /// only about not disturbing a board the re-read did not change — see
    /// [`Board::adopt`], whose answer is carried out of here.
    fn reload_boards(&mut self) -> bool {
        let open = self
            .board
            .and_then(|ix| self.boards.get(ix))
            .map(|board| board.id.clone());
        let mut held: HashMap<String, Board> = self
            .boards
            .drain(..)
            .map(|board| (board.id.clone(), board))
            .collect();
        let mut moved = false;
        self.boards = self
            .store()
            .boards()
            .into_iter()
            .map(|fresh| match held.remove(&fresh.id) {
                Some(mut board) => {
                    moved |= board.adopt(fresh);
                    board
                }
                None => fresh,
            })
            .collect();
        self.board = open.and_then(|id| self.boards.iter().position(|at| at.id == id));
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
        self.table = match open.and_then(|key| self.position(&key)) {
            found @ Some(_) => found,
            None => self.table.filter(|ix| *ix < self.tables.len()),
        };
        self.reload_page();
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.tables.iter().position(|table| table.key == key)
    }

    /// Read the open table's rows.
    pub fn reload_page(&mut self) {
        let key = self
            .table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone());
        self.page = match (key, self.data.as_ref()) {
            (Some(key), Some(data)) => data.read(&key, None, false, PAGE, 0).ok(),
            _ => None,
        };
    }

    /// Where this project's work is kept. The filesystem, for this app —
    /// [`schema::backend`] is what a different one would be, and nothing above
    /// here names a file.
    pub fn store(&self) -> backend::fs::Project {
        backend::fs::Project::new(&self.path)
    }

    /// The tab's label: the directory's own name, or the whole path when it
    /// has none (`/`).
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.sessions.iter().find(|chat| chat.id == id)
    }

    pub fn session_mut(&mut self, id: u64) -> Option<&mut ChatSession> {
        self.sessions.iter_mut().find(|chat| chat.id == id)
    }

    pub fn active_session(&self) -> Option<&ChatSession> {
        self.active.and_then(|id| self.session(id))
    }
}
