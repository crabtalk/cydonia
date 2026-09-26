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

use crate::{
    article::{self, Article, properties::Properties},
    board::{self, Board, key},
    entry, id,
    session::record::Record,
    stamp,
};
use anyhow::Result;
use notify::{RecursiveMode, Watcher as _};
use std::{
    cmp::Reverse,
    collections::HashSet,
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
};
use url::Url;

/// Everything cydonia holds for a project lives here: its articles, its
/// sessions, its boards and its database.
const DIR: &str = ".cydonia";

/// Where a project's boards live, and what the one board a project used to be
/// allowed was called.
const BOARDS: &str = "boards";
const BOARD_FILE: &str = "board.toml";

/// The project's SQL tables.
pub const DATA: &str = "data.db";

/// Where a project's sessions live. One file each, so writing one does not
/// rewrite the rest.
const SESSIONS: &str = "sessions";

/// A project on this disk.
#[derive(Clone)]
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

    /// Media that belongs to the project rather than to one article: what a
    /// session's messages carry, and the pictures in articles written before
    /// each held its own `assets/`.
    pub fn assets(&self) -> PathBuf {
        self.cydonia().join("assets")
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
        board.version = Some(version(body.as_bytes()));
        // A board written before ids existed already has one — the name of the
        // file it is in. Its columns and cards have none at all, and filling
        // those is [`Board::mint_ids`], which `boards` calls once it has the
        // whole list.
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
        // A new file, so there is nothing there to be checked against.
        board.version = None;
        if super::Project::save_board(self, &mut board).is_ok() {
            let _ = std::fs::remove_file(old);
        }
    }

    /// Every session file in this project by the id it is filed under,
    /// unread.
    pub fn session_files(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(self.sessions_dir()) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .map(|path| (stem(&path), path))
            .collect()
    }

    /// The `content.md` of an article that is here.
    fn article_file(&self, id: &str) -> Result<PathBuf> {
        let content = article::content(&article::dir(&self.root).join(component(id)?));
        anyhow::ensure!(content.is_file(), "no article {id}");
        Ok(content)
    }

    fn describe(&self, content: &Path) -> Article {
        let properties = article::properties::all(content);
        Article {
            id: article::id_of(content),
            title: properties.title,
            archived: properties.archived,
            touched: article::touched(content),
            cover: article::cover::of(content).and_then(|path| Url::from_file_path(path).ok()),
        }
    }

    fn sessions_dir(&self) -> PathBuf {
        self.cydonia().join(SESSIONS)
    }

    fn session_file(&self, id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{id}.json"))
    }
}

impl super::Project for Project {
    fn session(&self, id: &str) -> Option<Record> {
        let body = std::fs::read_to_string(self.session_file(id)).ok()?;
        let mut record: Record = serde_json::from_str(&body).ok()?;
        record.id = id.to_owned();
        record.number = self.number("session", id).ok();
        Some(record)
    }

    fn board(&self, id: &str) -> Option<Board> {
        let mut board = self.read_board(&self.board_file(id))?;
        board.number = self.number("board", id).ok();
        Some(board)
    }

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
        // A key is unique among a project's boards, so it is settled here,
        // where the whole list is in hand and nothing else can be holding one.
        let mut keys: HashSet<String> = boards
            .iter()
            .map(|board| board.key.clone())
            .filter(|key| !key.is_empty())
            .collect();
        // Anything short of ids is written back now rather than left for the
        // next save. Two reads of the same id-less board mint two different
        // sets, so a board that stayed unwritten would never compare equal to
        // itself and every re-read would report a change nobody made. The
        // write costs one watch event, which finds nothing left to mint.
        for board in &mut boards {
            board.number = self.number("board", &board.id).ok();
            let keyed = board.key.is_empty();
            if keyed {
                board.key = key::derive(&board.name, &keys);
                keys.insert(board.key.clone());
            }
            if board.mint_ids() || keyed {
                let _ = self.save_board(board);
            }
        }
        boards.sort_by_key(|board| Reverse(board.touched));
        boards
    }

    fn create_board(&self, name: &str, key: &str) -> Result<Board> {
        let dir = self.init()?.join(BOARDS);
        std::fs::create_dir_all(&dir)?;
        let mut board = Board::new(stem(&free(&dir, stamp::now())), name);
        board.key = match key::normalize(key) {
            Some(key) => key,
            // Nothing given, so it is derived from the name — against what the
            // project's other boards are already keyed, so it is clear of
            // them. Reading them is also what settles any key they are still
            // missing.
            None => {
                let taken: HashSet<String> =
                    self.boards().into_iter().map(|board| board.key).collect();
                key::derive(&board.name, &taken)
            }
        };
        board.number = self.number("board", &board.id).ok();
        self.save_board(&mut board)?;
        Ok(board)
    }

    /// The check and the write happen under an exclusive lock on the file,
    /// so two cydonia processes saving one board cannot both pass the check.
    /// The lock is advisory: a writer that does not take it is not stopped.
    fn save_board(&self, board: &mut Board) -> Result<()> {
        let body = toml::to_string_pretty(&*board)?;
        let path = self.board_file(&board.id);
        match &board.version {
            Some(seen) => {
                let mut file = match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Err(super::Stale.into());
                    }
                    Err(e) => return Err(e.into()),
                };
                file.lock()?;
                let mut held = Vec::new();
                file.read_to_end(&mut held)?;
                if version(&held) != *seen {
                    return Err(super::Stale.into());
                }
                file.set_len(0)?;
                file.seek(SeekFrom::Start(0))?;
                file.write_all(body.as_bytes())?;
            }
            None => std::fs::write(&path, &body)?,
        }
        board.touched = stamp::now();
        board.version = Some(version(body.as_bytes()));
        Ok(())
    }

    fn remove_board(&self, id: &str) -> Result<()> {
        std::fs::remove_file(self.board_file(id))?;
        self.retire("board", id)
    }

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
                record.number = self.number("session", &record.id).ok();
                Some(record)
            })
            .collect();
        found.sort_by_key(|record| Reverse(record.updated));
        found
    }

    /// Nothing is written: a session that never says anything leaves no file.
    ///
    /// Unique within this process — see [`stamp::fresh`] — and clear of any
    /// file already in the directory, which `-2` settles.
    fn create_session(&self) -> Result<String> {
        let dir = self.init()?.join(SESSIONS);
        std::fs::create_dir_all(&dir)?;
        let stamp = stamp::fresh();
        let mut id = stamp.to_string();
        for n in 2.. {
            if !dir.join(format!("{id}.json")).exists() {
                break;
            }
            id = format!("{stamp}-{n}");
        }
        Ok(id)
    }

    fn save_session(&self, record: &Record) -> Result<()> {
        let body = serde_json::to_string_pretty(record)?;
        std::fs::write(self.session_file(&record.id), body)?;
        Ok(())
    }

    fn remove_session(&self, id: &str) -> Result<()> {
        std::fs::remove_file(self.session_file(id))?;
        self.retire("session", id)
    }

    fn articles(&self) -> Vec<Article> {
        let Ok(entries) = std::fs::read_dir(article::dir(&self.root)) else {
            return Vec::new();
        };
        let mut found: Vec<Article> = entries
            .flatten()
            .map(|entry| article::content(&entry.path()))
            .filter(|content| content.is_file())
            .map(|content| self.describe(&content))
            .collect();
        found.sort_by_key(|article| Reverse(article.touched));
        found
    }

    fn article(&self, id: &str) -> Option<Article> {
        self.article_file(id)
            .ok()
            .map(|content| self.describe(&content))
    }

    fn create_article(&self, markdown: &str) -> Result<Article> {
        let dir = article::init(&self.root)?;
        let landing = article::free(&dir, stamp::now());
        std::fs::create_dir_all(&landing)?;
        let content = article::content(&landing);
        std::fs::write(&content, markdown)?;
        Ok(self.describe(&content))
    }

    fn read_article(&self, id: &str) -> Result<String> {
        Ok(std::fs::read_to_string(self.article_file(id)?)?)
    }

    fn write_article(&self, id: &str, markdown: &str) -> Result<()> {
        std::fs::write(self.article_file(id)?, markdown)?;
        Ok(())
    }

    fn properties(&self, id: &str) -> Properties {
        self.article_file(id)
            .map(|content| article::properties::all(&content))
            .unwrap_or_default()
    }

    fn save_properties(&self, id: &str, properties: &Properties) -> Result<()> {
        article::properties::save(&self.article_file(id)?, properties)?;
        Ok(())
    }

    fn remove_article(&self, id: &str) -> Result<()> {
        article::remove(&self.article_file(id)?)?;
        Ok(())
    }

    fn asset(&self, id: &str, name: &str) -> Result<Vec<u8>> {
        let content = self.article_file(id)?;
        Ok(std::fs::read(
            article::assets(&content).join(component(name)?),
        )?)
    }

    fn put_asset(&self, id: &str, name: &str, bytes: &[u8]) -> Result<()> {
        let dir = article::assets(&self.article_file(id)?);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(component(name)?), bytes)?;
        Ok(())
    }

    /// `.cydonia/` is made the first time a project keeps anything, which can
    /// be long after it was opened — so a project without one is watched
    /// shallowly at its own root, unsettled, where the one event that matters
    /// is the directory appearing.
    fn watch(&self, knock: impl Fn() + Send + Sync + 'static) -> Option<super::Watching> {
        // The prefix every event is matched against, resolved once. FSEvents
        // reports the real path, so a project reached through a symlink would
        // never match the prefix it was armed with.
        let dir = std::fs::canonicalize(&self.root)
            .unwrap_or_else(|_| self.root.clone())
            .join(DIR);
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event
                    && event.paths.iter().any(|path| ours(&dir, path))
                {
                    knock();
                }
            })
            .ok()?;
        match watcher.watch(&self.cydonia(), RecursiveMode::Recursive) {
            Ok(()) => Some(super::Watching::new(true, watcher)),
            Err(_) => watcher
                .watch(&self.root, RecursiveMode::NonRecursive)
                .ok()
                .map(|()| super::Watching::new(false, watcher)),
        }
    }

    fn number(&self, kind: &str, id: &str) -> Result<u64> {
        entry::Registry::open(&self.root)?.number(kind, id)
    }

    fn resolve(&self, kind: &str, number: u64) -> Result<Option<String>> {
        entry::Registry::open(&self.root)?.resolve(kind, number)
    }

    fn retire(&self, kind: &str, id: &str) -> Result<()> {
        entry::Registry::open(&self.root)?.remove(kind, id)
    }
}

/// The file's own name, which is what an entry made before ids was called.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map_or_else(id::mint, str::to_owned)
}

/// Whether a path that moved under `dir` (a project's `.cydonia/`) is one this
/// backend reads back.
///
/// Sessions are left out: the app writes a transcript on every frame of a
/// streaming turn, so a watch that covered them would be a watch on itself.
pub fn ours(dir: &Path, path: &Path) -> bool {
    let Ok(rest) = path.strip_prefix(dir) else {
        // Outside `.cydonia/`, where the only thing worth a knock is the
        // directory itself coming into existence.
        return path == dir;
    };
    let Some(head) = rest.components().next() else {
        return true;
    };
    let head = head.as_os_str().to_string_lossy();
    if head == article::DIR || head == BOARDS {
        return true;
    }
    // The database, and the log a commit actually lands in — it runs in WAL,
    // so the file itself only moves at a checkpoint. `-shm` is left out: the
    // reader's shared index is written on every read, so a watch on it would
    // knock on the app's own re-reads and never settle.
    let Some(tail) = head.strip_prefix(DATA) else {
        return false;
    };
    matches!(tail, "" | "-wal" | "-journal")
}

/// What a file held, as the version a save is checked against: a hash of the
/// bytes, so every build of cydonia reading one file agrees on it.
fn version(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// An id or asset name as one path component, refused where it would reach
/// outside the directory it names something in.
fn component(name: &str) -> Result<&str> {
    anyhow::ensure!(
        !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\']),
        "{name:?} is not a name"
    );
    Ok(name)
}

/// This millisecond's file, or the first after it that is not taken. Two
/// boards made inside one millisecond is the only way that happens.
fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(format!("{stamp}.toml")))
        .find(|board| !board.exists())
        .unwrap_or_else(|| dir.join(format!("{stamp}.toml")))
}
