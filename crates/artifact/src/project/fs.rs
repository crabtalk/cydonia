//! The filesystem backend: a project's own `.cydonia/`, one file per entry.
//!
//! A project here *is* a directory — it is what a session is spawned with as
//! its `cwd`, and moving it moves everything under it — so there is nothing to
//! open and nothing to close, and a [`Project`] is the path and no more.
//!
//! An entry's [`crate::id`] is the name of the file it is in, so nothing
//! here keeps a second map from one to the other — `boards/<id>.toml` is the
//! whole lookup, and a board handed back can be written again from its id
//! alone.
//!
//! Every write is best effort. A board that cannot be saved is not worth
//! failing a click over, and the caller has nothing better to do about it than
//! the person who can see the file does.

use crate::{
    board::{self, Board},
    id,
    session::record::Record,
    stamp,
};
use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
};

/// Everything cydonia holds for a project lives here: its articles, its
/// sessions, its boards and its database.
const DIR: &str = ".cydonia";

/// Where a project's boards live, and what the one board a project used to be
/// allowed was called.
const BOARDS: &str = "boards";
const BOARD_FILE: &str = "board.toml";

/// Where a project's sessions live. One file each, so writing one does not
/// rewrite the rest.
const SESSIONS: &str = "sessions";

/// A project on this disk.
pub struct Project {
    root: PathBuf,
}

impl Project {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory this project is, which is what a session runs in.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where this project's work is kept, whether or not any of it has been
    /// written yet.
    pub fn cydonia(&self) -> PathBuf {
        self.root.join(DIR)
    }

    /// The same directory, made if it is not there, and carrying the
    /// `.gitignore` that keeps the whole of it out of the repo it sits in —
    /// none of what cydonia writes here is the project's source.
    ///
    /// Every path that creates the directory comes through here. A second
    /// `create_dir_all` elsewhere would make it without the ignore file, and
    /// whichever ran first would decide whether the repo sees a database.
    pub fn init(&self) -> std::io::Result<PathBuf> {
        let dir = self.cydonia();
        std::fs::create_dir_all(&dir)?;
        let ignore = dir.join(".gitignore");
        if !ignore.exists() {
            std::fs::write(&ignore, "*\n")?;
        }
        Ok(dir)
    }

    fn boards_dir(&self) -> PathBuf {
        self.cydonia().join(BOARDS)
    }

    /// The file a board of this id is in. Derived rather than stored: the id
    /// is the name, so there is no second copy of it to disagree.
    fn board_file(&self, id: &str) -> PathBuf {
        self.boards_dir().join(format!("{id}.toml"))
    }

    fn read_board(&self, path: &Path) -> Option<Board> {
        let body = std::fs::read_to_string(path).ok()?;
        let mut board: Board = toml::from_str(&body).ok()?;
        board.touched = stamp::of(path);
        // A board written before ids existed already has one — the name of the
        // file it is in. Taken in memory and not written back: `boards` runs on
        // every re-read, and a write from inside one is an event the watch
        // would answer by re-reading again.
        if board.id.is_empty() {
            board.id = stem(path);
        }
        Some(board)
    }

    /// A project used to have one board, in `.cydonia/board.toml`. Give it the
    /// name and the directory the rest are made with, and it is the first of
    /// many.
    fn migrate_board(&self) {
        let old = self.cydonia().join(BOARD_FILE);
        let Some(mut board) = self.read_board(&old) else {
            return;
        };
        let to = self.boards_dir();
        if std::fs::create_dir_all(&to).is_err() {
            return;
        }
        board.id = stem(&free(&to, stamp::now()));
        board.name = board::NAMED.to_owned();
        super::Project::save_board(self, &mut board);
        let _ = std::fs::remove_file(old);
    }
}

impl Project {
    fn sessions_dir(&self) -> PathBuf {
        self.cydonia().join(SESSIONS)
    }

    fn session_file(&self, id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{id}.json"))
    }
}

impl super::Project for Project {
    fn boards(&self) -> Vec<Board> {
        self.migrate_board();
        let Ok(entries) = std::fs::read_dir(self.boards_dir()) else {
            return Vec::new();
        };
        let mut boards: Vec<Board> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .filter_map(|path| self.read_board(&path))
            .collect();
        boards.sort_by_key(|board| Reverse(board.touched));
        boards
    }
    fn create_board(&self) -> Option<Board> {
        let dir = self.init().ok()?.join(BOARDS);
        std::fs::create_dir_all(&dir).ok()?;
        let mut board = Board::new(stem(&free(&dir, stamp::now())), board::NAMED);
        super::Project::save_board(self, &mut board);
        Some(board)
    }
    /// Write a board back, and take the time it was written at — the key the
    /// sidebar orders on.
    fn save_board(&self, board: &mut Board) {
        let Ok(body) = toml::to_string_pretty(&*board) else {
            return;
        };
        if std::fs::write(self.board_file(&board.id), body).is_ok() {
            board.touched = stamp::now();
        }
    }
    fn remove_board(&self, id: &str) {
        let _ = std::fs::remove_file(self.board_file(id));
    }
    /// Every session filed in this project, most recently updated first.
    fn sessions(&self) -> Vec<Record> {
        let Ok(entries) = std::fs::read_dir(self.sessions_dir()) else {
            return Vec::new();
        };
        let mut found: Vec<Record> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| {
                let body = std::fs::read_to_string(&path).ok()?;
                let mut record: Record = serde_json::from_str(&body).ok()?;
                // Named by its file, for one written before ids existed.
                if record.id.is_empty() {
                    record.id = stem(&path);
                }
                Some(record)
            })
            .collect();
        found.sort_by_key(|record| Reverse(record.updated));
        found
    }
    /// Mint the id a session is filed under from here on. Called on the first
    /// write and not before: opening a project must not put a `.cydonia/` in
    /// it.
    ///
    /// Two sessions started inside one millisecond is the only collision, and
    /// `-2` is what settles it — the stamp is the same, so the pair still sort
    /// together.
    fn create_session(&self) -> Option<String> {
        let dir = self.init().ok()?.join(SESSIONS);
        std::fs::create_dir_all(&dir).ok()?;
        let stamp = stamp::now();
        let mut id = stamp.to_string();
        for n in 2.. {
            if !dir.join(format!("{id}.json")).exists() {
                break;
            }
            id = format!("{stamp}-{n}");
        }
        Some(id)
    }
    fn save_session(&self, record: &Record) {
        if let Ok(body) = serde_json::to_string_pretty(record) {
            let _ = std::fs::write(self.session_file(&record.id), body);
        }
    }
    fn remove_session(&self, id: &str) {
        let _ = std::fs::remove_file(self.session_file(id));
    }
}

/// The file's own name, which is what an entry made before ids was called.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map_or_else(id::mint, str::to_owned)
}

/// This millisecond's file, or the first after it that is not taken. Two
/// boards made inside one millisecond is the only way that happens.
fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(format!("{stamp}.toml")))
        .find(|board| !board.exists())
        .unwrap_or_else(|| dir.join(format!("{stamp}.toml")))
}
