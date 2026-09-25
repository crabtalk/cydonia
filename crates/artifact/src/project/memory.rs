//! A backend held in memory: nothing is read from or written to a disk, and
//! everything is gone when it is dropped.
//!
//! [`Project::seed`] fills one from the files of a `.cydonia/` in the layout
//! [`super::fs`] writes, so a fixture directory reads the same through either.
//! Tables (`data.db`) are not part of it. Article covers are not carried: an
//! [`Article`] from here has no cover.

use crate::{
    article::{
        Article,
        properties::{self, Properties},
    },
    board::{Board, key},
    id,
    session::record::Record,
    stamp,
};
use anyhow::{Result, anyhow, bail};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Mutex, MutexGuard},
};

#[derive(Default)]
pub struct Project(Mutex<State>);

#[derive(Default)]
struct State {
    boards: BTreeMap<String, Board>,
    sessions: BTreeMap<String, Record>,
    articles: BTreeMap<String, Held>,
    /// `(kind, id)` to the number issued to it.
    numbers: HashMap<(String, String), u64>,
    /// Every number issued, with the id it is on, or `None` once retired.
    issued: BTreeMap<u64, (String, Option<String>)>,
}

#[derive(Default)]
struct Held {
    markdown: String,
    /// The text of its `properties.toml`, kept whole so keys this crate does
    /// not know about survive a save.
    properties: String,
    assets: BTreeMap<String, Vec<u8>>,
    touched: u128,
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// A backend holding the files of a `.cydonia/`, each named by its path
    /// relative to that directory with `/` between components. Files it does
    /// not read are skipped, as is anything that does not parse.
    pub fn seed<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Self {
        let this = Self::new();
        {
            let mut state = this.state();
            for (path, bytes) in files {
                let parts: Vec<&str> = path.split('/').collect();
                let text = || String::from_utf8_lossy(bytes).into_owned();
                match parts.as_slice() {
                    ["boards", file] => {
                        let Some(stem) = file.strip_suffix(".toml") else {
                            continue;
                        };
                        let Ok(mut board) = toml::from_str::<Board>(&text()) else {
                            continue;
                        };
                        if board.id.is_empty() {
                            board.id = stem.to_owned();
                        }
                        state.boards.insert(board.id.clone(), board);
                    }
                    ["sessions", file] => {
                        let Some(stem) = file.strip_suffix(".json") else {
                            continue;
                        };
                        let Ok(mut record) = serde_json::from_str::<Record>(&text()) else {
                            continue;
                        };
                        if record.id.is_empty() {
                            record.id = stem.to_owned();
                        }
                        state.sessions.insert(record.id.clone(), record);
                    }
                    ["articles", article, "content.md"] => {
                        state
                            .articles
                            .entry((*article).to_owned())
                            .or_default()
                            .markdown = text();
                    }
                    ["articles", article, "properties.toml"] => {
                        state
                            .articles
                            .entry((*article).to_owned())
                            .or_default()
                            .properties = text();
                    }
                    ["articles", article, "assets", name] => {
                        state
                            .articles
                            .entry((*article).to_owned())
                            .or_default()
                            .assets
                            .insert((*name).to_owned(), bytes.to_vec());
                    }
                    _ => {}
                }
            }
            let now = stamp::now();
            for held in state.articles.values_mut() {
                held.touched = now;
            }
            let mut keys: HashSet<String> = state
                .boards
                .values()
                .map(|board| board.key.clone())
                .filter(|key| !key.is_empty())
                .collect();
            for board in state.boards.values_mut() {
                board.touched = now;
                if board.key.is_empty() {
                    board.key = key::derive(&board.name, &keys);
                    keys.insert(board.key.clone());
                }
                board.mint_ids();
            }
        }
        this
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl State {
    fn number(&mut self, kind: &str, id: &str) -> u64 {
        let key = (kind.to_owned(), id.to_owned());
        if let Some(number) = self.numbers.get(&key) {
            return *number;
        }
        let number = self.issued.keys().next_back().map_or(1, |last| last + 1);
        self.issued
            .insert(number, (kind.to_owned(), Some(id.to_owned())));
        self.numbers.insert(key, number);
        number
    }

    fn retire(&mut self, kind: &str, id: &str) {
        if let Some(number) = self.numbers.remove(&(kind.to_owned(), id.to_owned()))
            && let Some(entry) = self.issued.get_mut(&number)
        {
            entry.1 = None;
        }
    }

    fn article(&mut self, id: &str) -> Result<&mut Held> {
        self.articles
            .get_mut(id)
            .ok_or_else(|| anyhow!("no article {id}"))
    }

    fn describe(&self, id: &str, held: &Held) -> Article {
        let properties = properties::parse(&held.properties);
        Article {
            id: id.to_owned(),
            title: properties.title,
            archived: properties.archived,
            touched: held.touched,
            cover: None,
        }
    }
}

impl super::Project for Project {
    fn boards(&self) -> Vec<Board> {
        let mut state = self.state();
        let ids: Vec<String> = state.boards.keys().cloned().collect();
        let mut boards: Vec<Board> = ids
            .into_iter()
            .map(|id| {
                let number = state.number("board", &id);
                let mut board = state.boards[&id].clone();
                board.number = Some(number);
                board
            })
            .collect();
        boards.sort_by_key(|board| Reverse(board.touched));
        boards
    }

    fn board(&self, id: &str) -> Option<Board> {
        let mut state = self.state();
        let mut board = state.boards.get(id)?.clone();
        board.number = Some(state.number("board", id));
        Some(board)
    }

    fn create_board(&self, name: &str, key: &str) -> Result<Board> {
        let mut state = self.state();
        let mut board = Board::new(id::mint(), name);
        board.key = match key::normalize(key) {
            Some(key) => key,
            None => {
                let taken: HashSet<String> = state
                    .boards
                    .values()
                    .map(|board| board.key.clone())
                    .collect();
                key::derive(&board.name, &taken)
            }
        };
        board.touched = stamp::now();
        board.number = Some(state.number("board", &board.id));
        state.boards.insert(board.id.clone(), board.clone());
        Ok(board)
    }

    fn save_board(&self, board: &mut Board) -> Result<()> {
        board.touched = stamp::now();
        self.state().boards.insert(board.id.clone(), board.clone());
        Ok(())
    }

    fn remove_board(&self, id: &str) -> Result<()> {
        let mut state = self.state();
        state
            .boards
            .remove(id)
            .ok_or_else(|| anyhow!("no board {id}"))?;
        state.retire("board", id);
        Ok(())
    }

    fn sessions(&self) -> Vec<Record> {
        let mut state = self.state();
        let ids: Vec<String> = state.sessions.keys().cloned().collect();
        let mut found: Vec<Record> = ids
            .into_iter()
            .map(|id| {
                let number = state.number("session", &id);
                let mut record = state.sessions[&id].clone();
                record.number = Some(number);
                record
            })
            .collect();
        found.sort_by_key(|record| Reverse(record.updated));
        found
    }

    fn session(&self, id: &str) -> Option<Record> {
        let mut state = self.state();
        let mut record = state.sessions.get(id)?.clone();
        record.number = Some(state.number("session", id));
        Some(record)
    }

    fn create_session(&self) -> Result<String> {
        Ok(stamp::fresh().to_string())
    }

    fn save_session(&self, record: &Record) -> Result<()> {
        self.state()
            .sessions
            .insert(record.id.clone(), record.clone());
        Ok(())
    }

    fn remove_session(&self, id: &str) -> Result<()> {
        let mut state = self.state();
        state
            .sessions
            .remove(id)
            .ok_or_else(|| anyhow!("no session {id}"))?;
        state.retire("session", id);
        Ok(())
    }

    fn articles(&self) -> Vec<Article> {
        let state = self.state();
        let mut found: Vec<Article> = state
            .articles
            .iter()
            .map(|(id, held)| state.describe(id, held))
            .collect();
        found.sort_by_key(|article| Reverse(article.touched));
        found
    }

    fn create_article(&self, markdown: &str) -> Result<Article> {
        let mut state = self.state();
        let id = (stamp::now()..)
            .map(|stamp| stamp.to_string())
            .find(|id| !state.articles.contains_key(id))
            .unwrap_or_else(id::mint);
        let held = Held {
            markdown: markdown.to_owned(),
            touched: stamp::now(),
            ..Held::default()
        };
        let article = state.describe(&id, &held);
        state.articles.insert(id, held);
        Ok(article)
    }

    fn read_article(&self, id: &str) -> Result<String> {
        Ok(self.state().article(id)?.markdown.clone())
    }

    fn write_article(&self, id: &str, markdown: &str) -> Result<()> {
        let mut state = self.state();
        let held = state.article(id)?;
        held.markdown = markdown.to_owned();
        held.touched = stamp::now();
        Ok(())
    }

    fn properties(&self, id: &str) -> Properties {
        self.state()
            .articles
            .get(id)
            .map(|held| properties::parse(&held.properties))
            .unwrap_or_default()
    }

    fn save_properties(&self, id: &str, properties: &Properties) -> Result<()> {
        let mut state = self.state();
        let held = state.article(id)?;
        held.properties = properties::apply(&held.properties, properties).unwrap_or_default();
        held.touched = stamp::now();
        Ok(())
    }

    fn remove_article(&self, id: &str) -> Result<()> {
        let mut state = self.state();
        state
            .articles
            .remove(id)
            .ok_or_else(|| anyhow!("no article {id}"))?;
        state.retire("article", id);
        Ok(())
    }

    fn asset(&self, id: &str, name: &str) -> Result<Vec<u8>> {
        let mut state = self.state();
        state
            .article(id)?
            .assets
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("no asset {name} in article {id}"))
    }

    fn put_asset(&self, id: &str, name: &str, bytes: &[u8]) -> Result<()> {
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            bail!("{name:?} is not a name");
        }
        let mut state = self.state();
        state
            .article(id)?
            .assets
            .insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn number(&self, kind: &str, id: &str) -> Result<u64> {
        Ok(self.state().number(kind, id))
    }

    fn resolve(&self, kind: &str, number: u64) -> Result<Option<String>> {
        Ok(self
            .state()
            .issued
            .get(&number)
            .filter(|(issued, _)| issued == kind)
            .and_then(|(_, id)| id.clone()))
    }

    fn retire(&self, kind: &str, id: &str) -> Result<()> {
        self.state().retire(kind, id);
        Ok(())
    }
}
