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
use artifact::{
    board::Board,
    project::{Project as _, fs},
};
use bezel::gpui::Context;
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
            boards: fs::Project::new(&path).boards(),
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
            expanded: true,
            archive_open: false,
            watch: None,
        };
        this.reload_tables();
        this.unload_boards(None);
        this
    }

    pub fn load_board(&mut self, id: &str) -> bool {
        if !self.unloaded_boards.contains(id) {
            return true;
        }
        let Some(fresh) = self.store().board(id) else {
            return false;
        };
        let Some(board) = self.boards.iter_mut().find(|board| board.id == id) else {
            return false;
        };
        *board = fresh;
        self.unloaded_boards.remove(id);
        true
    }

    pub fn unload_boards(&mut self, visible: Option<usize>) {
        let store = self.store();
        for (ix, board) in self.boards.iter_mut().enumerate() {
            if !board.archived || visible == Some(ix) || self.unloaded_boards.contains(&board.id) {
                continue;
            }
            let Some(saved) = store.board(&board.id) else {
                continue;
            };
            if serde_json::to_value(&saved).ok() != serde_json::to_value(&*board).ok() {
                continue;
            }
            board.columns = Vec::new();
            self.unloaded_boards.insert(board.id.clone());
        }
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
        self.unloaded_boards.clear();
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
        self.unload_boards(self.board);
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
    /// The page the open table is on.
    pub fn open_page(&self) -> Option<&Page> {
        let key = &self.tables.get(self.table?)?.key;
        self.pages.get(key)
    }

    /// Read the rows for the open table. Any other page is dropped: a page is
    /// a window on a table nobody is looking at.
    pub fn reload_page(&mut self) {
        let wanted: Vec<String> = self
            .table
            .and_then(|ix| self.tables.get(ix))
            .map(|table| table.key.clone())
            .into_iter()
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

    /// Where this project's work is kept. The filesystem, for this app —
    /// [`artifact::project::Project`] is what a different one would answer, and
    /// nothing above here names a file.
    pub fn store(&self) -> fs::Project {
        fs::Project::new(&self.path)
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

#[cfg(test)]
#[path = "../../tests/unit/archive_boards.rs"]
mod archive_tests;
