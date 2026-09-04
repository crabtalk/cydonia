//! A project: a directory, the sessions running in it, its boards and its
//! articles.
//!
//! The path is the whole identity — it is what every session in the project
//! is spawned with as its `cwd`, and what [`crate::model::state`] persists.

use crate::{
    data::{Data, Page, Table},
    model::{
        article::{self, Article},
        board::{self, Board},
        session::ChatSession,
    },
};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Everything cydonia holds for a project lives here: its articles, its
/// archived sessions, and its database.
const DIR: &str = ".cydonia";

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
}

impl Project {
    pub fn new(path: PathBuf) -> Self {
        let mut this = Self {
            boards: board::list(&path),
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
        };
        this.reload_tables();
        this
    }

    /// Re-read what tables exist. The store is the list — nothing here keeps a
    /// second copy of it that a failed write could leave standing.
    pub fn reload_tables(&mut self) {
        // Held by key across the re-read, not by index: the list is ordered by
        // name, so renaming the open table moves it and an index would leave
        // the pane showing whichever table slid into its place.
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

    /// Open sessions and archived ones, in that order — an archived session
    /// is history and belongs under the work still going on.
    pub fn ordered(&self) -> impl Iterator<Item = &ChatSession> {
        let open = self.sessions.iter().filter(|chat| !chat.closed);
        open.chain(self.sessions.iter().filter(|chat| chat.closed))
    }
}

/// Now, in milliseconds — the id an article or a board is made with. Sorting
/// these is sorting by age, which is the order they are listed back in.
pub fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

/// When a file was last written, as the same millisecond stamp ids carry — the
/// key entries are listed by, so the one you touched last is the one on top.
pub fn written(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(stamp, |since| since.as_millis())
}

pub fn dir(project: &Path) -> PathBuf {
    project.join(DIR)
}

/// The same directory, made if it is not there, and carrying the `.gitignore`
/// that keeps the whole of it out of the repo it sits in — none of what cydonia
/// writes here is the project's source.
///
/// Every path that creates the directory comes through here. A second
/// `create_dir_all` elsewhere would make it without the ignore file, and
/// whichever ran first would decide whether the repo sees a database.
pub fn init(project: &Path) -> std::io::Result<PathBuf> {
    let dir = dir(project);
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    Ok(dir)
}
