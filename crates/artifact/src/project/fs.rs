//! The filesystem backend: a project's own `.cydonia/`, one file per entry.
//!
//! A project here *is* a directory — it is what a session is spawned with as
//! its `cwd`, and moving it moves everything under it — so there is nothing to
//! open and nothing to close, and a [`Project`] is the path and no more.
//!
//! An entry's [`crate::id`] is the name of the file or directory it is in, so
//! nothing here keeps a second map from one to the other — `boards/<id>/` is
//! the whole lookup, and a board handed back can be written again from its id
//! alone.

use super::layout;
#[cfg(feature = "sqlite")]
use crate::entry;
use crate::{
    article::{self, Article, properties::Properties},
    board::{self, Board, key},
    document, id,
    session::record::Record,
    space::Kind,
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

/// Where a project's boards live, a directory each — see [`super::layout`] —
/// and what the one board a project used to be allowed was called.
const BOARDS: &str = "boards";
const LEGACY_BOARD: &str = "board.toml";

/// The project's SQL tables.
pub const DATA: &str = "data.db";

/// The project's bookkeeping: entry numbers.
pub const STATE: &str = "state.db";

/// What [`STATE`] was called before.
const ENTRIES: &str = "entries.db";

/// The release whose files the migrations here replace — the backup they are
/// kept under; see [`crate::backup`].
const REPLACED: &str = "v0_1_26";

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

    /// [`STATE`], made if it is not there. A project still carrying
    /// `entries.db` has it renamed into place first.
    pub fn state(&self) -> std::io::Result<PathBuf> {
        let dir = self.init()?;
        let state = dir.join(STATE);
        let entries = dir.join(ENTRIES);
        if !state.exists() && entries.exists() {
            let _ = crate::backup::keep(REPLACED, &self.root, Path::new(ENTRIES));
            match std::fs::rename(&entries, &state) {
                // Another connection renamed it first.
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                result => result?,
            }
        }
        Ok(state)
    }

    fn boards_dir(&self) -> PathBuf {
        self.cydonia().join(BOARDS)
    }

    /// The directory a board of this id is in. Derived rather than stored:
    /// the id is the name, so there is no second copy of it to disagree.
    fn board_dir(&self, id: &str) -> PathBuf {
        self.boards_dir().join(id)
    }

    /// A board as it is on disk, in either layout: its directory, or a flat
    /// `boards/<id>.toml` written before boards had directories.
    fn read_board(&self, path: &Path) -> Option<Board> {
        match path.is_dir() {
            true => self.read_board_dir(path),
            false => read_flat_board(path),
        }
    }

    fn read_board_dir(&self, dir: &Path) -> Option<Board> {
        let file = dir.join(layout::BOARD_FILE);
        let body = std::fs::read_to_string(&file).ok()?;
        let mut touched = stamp::of(&file);
        let mut cards = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir.join(layout::CARDS)) {
            for card_dir in entries.flatten().map(|entry| entry.path()) {
                let path = document::content(&card_dir);
                let Some((text, properties)) = read_card_files(&path) else {
                    continue;
                };
                let id = card_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut card = layout::parse_card(&id, &text, &properties);
                card.version = Some(card_version(&text, &properties));
                touched = touched.max(document::touched(&path));
                cards.push(card);
            }
        }
        let mut board = layout::assemble(&body, cards)?;
        if board.id.is_empty() {
            board.id = stem(dir);
        }
        board.touched = touched;
        board.version = Some(version(body.as_bytes()));
        Some(board)
    }

    /// A project used to have one board, in `.cydonia/board.toml`. Give it the
    /// name and the directory the rest are made with, and it is the first of
    /// many.
    fn migrate_board(&self) {
        let old = self.cydonia().join(LEGACY_BOARD);
        let Some(mut board) = read_flat_board(&old) else {
            return;
        };
        let _ = crate::backup::keep(REPLACED, &self.root, Path::new(LEGACY_BOARD));
        if std::fs::create_dir_all(self.boards_dir()).is_err() {
            return;
        }
        board.id = free_board(&self.boards_dir(), stamp::now());
        board.name = board::NAMED.to_owned();
        board.mint_ids();
        self.adopt_assets(&mut board);
        if self.write_board(&mut board).is_ok() {
            let _ = crate::backup::note_legacy_board(REPLACED, &self.root, &board.id);
            let _ = std::fs::remove_file(old);
        }
    }

    /// Move a flat `boards/<id>.toml` into its directory. One whose directory
    /// already exists is left where it is.
    fn migrate_flat(&self, flat: &Path) {
        let id = stem(flat);
        if self.board_dir(&id).exists() {
            return;
        }
        let Some(mut board) = read_flat_board(flat) else {
            return;
        };
        let _ = crate::backup::keep(
            REPLACED,
            &self.root,
            &Path::new(BOARDS).join(format!("{id}.toml")),
        );
        board.id = id;
        board.mint_ids();
        self.adopt_assets(&mut board);
        if self.write_board(&mut board).is_ok() {
            let _ = std::fs::remove_file(flat);
        }
    }

    /// Copy the pictures a card points at in the project's `.cydonia/assets/`
    /// into the card's own `assets/`, and point the card at the copies — see
    /// [`document::adopt_assets`].
    fn adopt_assets(&self, board: &mut Board) {
        let shared = self.assets();
        let dir = self.board_dir(&board.id);
        for card in board
            .columns
            .iter_mut()
            .flat_map(|column| &mut column.cards)
        {
            let Ok(id) = component(&card.id) else {
                continue;
            };
            let own = document::assets(&document::content(&dir.join(layout::CARDS).join(id)));
            card.text = document::adopt_assets(&card.text, &shared, &own);
        }
    }

    /// The directory a card is — see [`layout`].
    pub fn card_dir(&self, board: &str, card: &str) -> PathBuf {
        self.board_dir(board).join(layout::CARDS).join(card)
    }

    /// Copy a card's directory to where it is landing on another board, which
    /// may be in another project, so its pictures go with it. Run before
    /// either board is saved: the source's save removes the card's directory.
    pub fn carry_card_files(
        &self,
        board: &str,
        card: &str,
        to: &Project,
        to_board: &str,
        to_card: &str,
    ) -> Result<()> {
        let from = self.card_dir(component(board)?, component(card)?);
        if !from.is_dir() {
            return Ok(());
        }
        let landing = to.card_dir(component(to_board)?, component(to_card)?);
        document::copy_dir(&from, &landing)?;
        Ok(())
    }

    fn migrate_flats(&self) {
        let Ok(entries) = std::fs::read_dir(self.boards_dir()) else {
            return;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                self.migrate_flat(&path);
            }
        }
    }

    /// Write a board into its directory under an exclusive lock on its
    /// `board.toml`, so two cydonia processes saving one board cannot both
    /// pass the check. The lock is advisory.
    ///
    /// `board.toml` is checked against [`Board::version`] only when this save
    /// changes it; a card file is checked against [`board::Card::version`]
    /// only when this save changes that card. Card files are written before
    /// `board.toml`, and the files of cards it no longer lists are removed
    /// after it.
    fn write_board(&self, board: &mut Board) -> Result<()> {
        let dir = self.board_dir(component(&board.id)?);
        let file = dir.join(layout::BOARD_FILE);
        let cards_dir = dir.join(layout::CARDS);
        if board.version.is_some() && !file.is_file() {
            return Err(super::Stale.into());
        }
        std::fs::create_dir_all(&cards_dir)?;
        let structure = layout::board_toml(board)?;
        let structure_version = version(structure.as_bytes());
        let mut lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&file)?;
        lock.lock()?;
        let mut held = String::new();
        lock.read_to_string(&mut held)?;
        let ours = board.version.as_deref() != Some(structure_version.as_str());
        let theirs = board
            .version
            .as_ref()
            .is_some_and(|seen| *seen != version(held.as_bytes()));
        if ours && theirs {
            return Err(super::Stale.into());
        }
        let held_ids: HashSet<String> = layout::card_ids(&held);
        let mut writes = Vec::new();
        for card in board.columns.iter().flat_map(|column| &column.cards) {
            if !ours && !held_ids.contains(&card.id) {
                return Err(super::Stale.into());
            }
            let path = document::content(&cards_dir.join(component(&card.id)?));
            let held = read_card_files(&path);
            let held_properties = held.as_ref().map_or("", |(_, properties)| properties);
            let properties = layout::card_properties(card, held_properties);
            let written = card_version(&card.text, properties.as_deref().unwrap_or(""));
            if card.version.as_deref() == Some(written.as_str()) {
                continue;
            }
            if let Some(seen) = &card.version
                && held
                    .as_ref()
                    .map(|(text, properties)| card_version(text, properties))
                    .as_ref()
                    != Some(seen)
            {
                return Err(super::Stale.into());
            }
            writes.push((
                card.id.clone(),
                path,
                card.text.clone(),
                properties,
                written,
            ));
        }
        for (_, path, text, properties, _) in &writes {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, text)?;
            let beside = document::properties(path);
            match properties {
                Some(properties) => std::fs::write(&beside, properties)?,
                None => {
                    let _ = std::fs::remove_file(&beside);
                }
            }
        }
        if ours {
            lock.set_len(0)?;
            lock.seek(SeekFrom::Start(0))?;
            lock.write_all(structure.as_bytes())?;
            let kept: HashSet<&str> = board
                .columns
                .iter()
                .flat_map(|column| &column.cards)
                .map(|card| card.id.as_str())
                .collect();
            for gone in held_ids.iter().filter(|id| !kept.contains(id.as_str())) {
                if let Ok(gone) = component(gone) {
                    let _ = std::fs::remove_dir_all(cards_dir.join(gone));
                }
            }
            board.version = Some(structure_version);
        }
        for card in board
            .columns
            .iter_mut()
            .flat_map(|column| &mut column.cards)
        {
            if let Some((.., written)) = writes.iter().find(|(id, ..)| *id == card.id) {
                card.version = Some(written.clone());
            }
        }
        board.touched = stamp::now();
        Ok(())
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

    /// Every article's `content.md` in this project by the article's id,
    /// unread.
    pub fn article_files(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(article::dir(&self.root)) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| article::content(&entry.path()))
            .filter(|content| content.is_file())
            .map(|content| (article::id_of(&content), content))
            .collect()
    }

    /// Every board directory in this project by the board's id, unread.
    pub fn board_files(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(self.boards_dir()) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.join(layout::BOARD_FILE).is_file())
            .map(|path| (stem(&path), path))
            .collect()
    }

    /// One board as it is on disk, from its directory: ids it lacks are not
    /// minted and nothing is written back.
    pub fn read_board_file(&self, path: &Path) -> Option<Board> {
        self.read_board(path)
    }

    /// The article whose `content.md` this is.
    pub fn describe_article(&self, content: &Path) -> Option<Article> {
        content.is_file().then(|| self.describe(content))
    }

    /// Where an entry is on disk: an article's directory, a board's or a
    /// session's file. `None` for one that is not there, and for a table,
    /// whose rows are in the project's database.
    pub fn place(&self, kind: Kind, id: &str) -> Option<PathBuf> {
        let path = match kind {
            Kind::Article => self.article_file(id).ok()?.parent()?.to_path_buf(),
            Kind::Board => self.board_dir(component(id).ok()?),
            Kind::Session => self.session_file(component(id).ok()?),
            Kind::Table => return None,
        };
        path.exists().then_some(path)
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
            cover: cover_url(content),
            labels: properties.labels,
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
        let id = component(id).ok()?;
        let flat = self.boards_dir().join(format!("{id}.toml"));
        if flat.is_file() {
            self.migrate_flat(&flat);
        }
        let mut board = self.read_board_dir(&self.board_dir(id))?;
        board.number = self.number("board", id).ok();
        Some(board)
    }

    fn boards(&self) -> Vec<Board> {
        self.migrate_board();
        self.migrate_flats();
        let Ok(entries) = std::fs::read_dir(self.boards_dir()) else {
            return Vec::new();
        };
        let mut boards: Vec<Board> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| self.read_board_dir(&path))
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
        let mut board = Board::new(free_board(&dir, stamp::now()), name);
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

    fn save_board(&self, board: &mut Board) -> Result<()> {
        self.write_board(board)
    }

    fn remove_board(&self, id: &str) -> Result<()> {
        std::fs::remove_dir_all(self.board_dir(component(id)?))?;
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
        let article = self.describe(&content);
        Ok(article)
    }

    fn read_article(&self, id: &str) -> Result<String> {
        Ok(std::fs::read_to_string(self.article_file(id)?)?)
    }

    fn write_article(&self, id: &str, markdown: &str) -> Result<()> {
        let path = self.article_file(id)?;
        std::fs::write(&path, markdown)?;
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
        #[cfg(feature = "sqlite")]
        return entry::Registry::open(&self.root)?.number(kind, id);
        #[cfg(not(feature = "sqlite"))]
        no_numbers(kind, id)
    }

    fn resolve(&self, kind: &str, number: u64) -> Result<Option<String>> {
        #[cfg(feature = "sqlite")]
        return entry::Registry::open(&self.root)?.resolve(kind, number);
        #[cfg(not(feature = "sqlite"))]
        no_numbers(kind, &number.to_string())
    }

    fn retire(&self, kind: &str, id: &str) -> Result<()> {
        #[cfg(feature = "sqlite")]
        return entry::Registry::open(&self.root)?.remove(kind, id);
        #[cfg(not(feature = "sqlite"))]
        no_numbers(kind, id)
    }
}

/// The file's own name, which is what an entry made before ids was called.
fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map_or_else(id::mint, str::to_owned)
}

#[cfg(not(feature = "sqlite"))]
fn no_numbers<T>(kind: &str, id: &str) -> Result<T> {
    anyhow::bail!("no number for {kind} {id}: built without the sqlite feature")
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

/// The cover beside a document, as the `file://` it is. wasm32 has no file
/// URLs, and no cover.
#[cfg(not(target_family = "wasm"))]
fn cover_url(content: &Path) -> Option<Url> {
    article::cover::of(content).and_then(|path| Url::from_file_path(path).ok())
}

#[cfg(target_family = "wasm")]
fn cover_url(_: &Path) -> Option<Url> {
    None
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
/// A card's `content.md` and its `properties.toml`, empty where that file is
/// missing. `None` for a card with no `content.md`.
fn read_card_files(content: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(content).ok()?;
    let properties = std::fs::read_to_string(document::properties(content)).unwrap_or_default();
    Some((text, properties))
}

/// What a card's two files hold, as one version.
fn card_version(text: &str, properties: &str) -> String {
    version(format!("{text}\0{properties}").as_bytes())
}

/// A board id no directory or flat file under `dir` is using.
fn free_board(dir: &Path, stamp: u128) -> String {
    (stamp..)
        .map(|stamp| stamp.to_string())
        .find(|id| !dir.join(id).exists() && !dir.join(format!("{id}.toml")).exists())
        .unwrap_or_else(|| stamp.to_string())
}

/// A board written whole into one TOML file, its cards inline: the layout
/// before boards had directories.
fn read_flat_board(path: &Path) -> Option<Board> {
    let body = std::fs::read_to_string(path).ok()?;
    let mut board: Board = toml::from_str(&body).ok()?;
    board.touched = stamp::of(path);
    if board.id.is_empty() {
        board.id = stem(path);
    }
    Some(board)
}
