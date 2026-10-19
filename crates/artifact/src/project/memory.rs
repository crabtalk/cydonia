//! A backend held in memory: nothing is read from or written to a disk, and
//! everything is gone when it is dropped.
//!
//! [`Project::seed`] fills one from the files of a `.cydonia/` in the layout
//! [`super::fs`] writes, so a fixture directory reads the same through either.
//! Tables (`data.db`) are not part of it. Article covers are not carried: an
//! [`Article`] from here has no cover.

use crate::error::Error;
use crate::kind::Kind;
use crate::{
    article::{
        Article,
        comments::Comments,
        properties::{self, Properties},
    },
    board::{Board, Page, key},
    id,
    project::{Owner, fs::component, layout},
    session::record::Record,
    stamp,
};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
    future::ready,
    sync::{Mutex, MutexGuard},
};

/// A seeded board directory: its `board.toml`, and each card's `content.md`
/// and `properties.toml` by the card's id.
type Seeded = (String, BTreeMap<String, (String, String)>, String);

#[derive(Default)]
pub struct Project(Mutex<State>);

#[derive(Default)]
struct State {
    boards: BTreeMap<String, Board>,
    /// Each board's version, `0` for one never saved.
    versions: HashMap<String, String>,
    /// How many saves of a board have been made here: what the next one's
    /// version is spelled from.
    saves: u64,
    sessions: BTreeMap<String, Record>,
    articles: BTreeMap<String, Held>,
    /// Each owner's pictures, by name.
    assets: HashMap<Owner, BTreeMap<String, Vec<u8>>>,
    /// `(kind, id)` to the number issued to it.
    numbers: HashMap<(Kind, String), u64>,
    /// Every number issued, with the id it is on, or `None` once retired.
    issued: BTreeMap<u64, (Kind, Option<String>)>,
}

#[derive(Default)]
struct Held {
    markdown: String,
    /// The text of its `properties.toml`, kept whole so keys this crate does
    /// not know about survive a save.
    properties: String,
    /// Its comments, held against `markdown`.
    comments: Comments,
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
            // A board directory's `board.toml` and its cards' files, assembled
            // once every file is in.
            let mut dirs: BTreeMap<String, Seeded> = BTreeMap::new();
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
                        board.created = stamp::named(stem);
                        state.boards.insert(board.id.clone(), board);
                    }
                    ["boards", id, "board.toml"] => {
                        dirs.entry((*id).to_owned()).or_default().0 = text();
                    }
                    ["boards", id, "content.md"] => {
                        dirs.entry((*id).to_owned()).or_default().2 = text();
                    }
                    ["boards", id, "assets", name] => {
                        let owner = Owner::Page {
                            board: (*id).to_owned(),
                            page: Page::Description,
                        };
                        state.hold_asset(owner, name, bytes);
                    }
                    ["boards", id, "cards", card, "assets", name] => {
                        let owner = Owner::Page {
                            board: (*id).to_owned(),
                            page: Page::Card((*card).to_owned()),
                        };
                        state.hold_asset(owner, name, bytes);
                    }
                    ["assets", name] => state.hold_asset(Owner::Shared, name, bytes),
                    [
                        "boards",
                        id,
                        "cards",
                        card,
                        file @ ("content.md" | "properties.toml"),
                    ] => {
                        let held = dirs
                            .entry((*id).to_owned())
                            .or_default()
                            .1
                            .entry((*card).to_owned())
                            .or_default();
                        match *file {
                            "content.md" => held.0 = text(),
                            _ => held.1 = text(),
                        }
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
                        state.hold_asset(Owner::Article((*article).to_owned()), name, bytes);
                    }
                    _ => {}
                }
            }
            for (id, (body, cards, description)) in dirs {
                let cards = cards
                    .iter()
                    .map(|(card, (content, properties))| {
                        layout::parse_card(card, content, properties)
                    })
                    .collect();
                let Some(mut board) = layout::assemble(&body, cards) else {
                    continue;
                };
                if board.id.is_empty() {
                    board.id = id.clone();
                }
                board.created = stamp::named(&id);
                board.description = description;
                state.boards.insert(board.id.clone(), board);
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

    /// Holds `board` as another backend made it: its number, version and the
    /// time it was written are kept as they are.
    pub fn hold_board(&self, board: Board) {
        let mut state = self.state();
        if let Some(number) = board.number {
            state.hold_number(Kind::Board, &board.id, number);
        }
        match &board.version {
            Some(version) => state.versions.insert(board.id.clone(), version.clone()),
            None => state.versions.remove(&board.id),
        };
        state.boards.insert(board.id.clone(), board);
    }

    /// Holds `record` as another backend made it, its number kept.
    pub fn hold_session(&self, record: Record) {
        let mut state = self.state();
        if let Some(number) = record.number {
            state.hold_number(Kind::Session, &record.id, number);
        }
        state.sessions.insert(record.id.clone(), record);
    }

    /// Holds an article as another backend made it: `article`'s number and
    /// the time it was touched are kept, and its title, archived flag and
    /// labels are `properties`'.
    pub fn hold_article(
        &self,
        article: &Article,
        markdown: String,
        properties: &Properties,
        comments: Comments,
    ) {
        let mut state = self.state();
        if let Some(number) = article.number {
            state.hold_number(Kind::Article, &article.id, number);
        }
        let held = state.articles.entry(article.id.clone()).or_default();
        held.markdown = markdown;
        held.properties = properties::apply("", properties).unwrap_or_default();
        held.comments = comments;
        held.touched = article.touched;
    }

    /// Holds `number` as issued elsewhere to `id`.
    pub fn hold_number(&self, kind: Kind, id: &str, number: u64) {
        self.state().hold_number(kind, id, number);
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl State {
    fn number(&mut self, kind: Kind, id: &str) -> u64 {
        let key = (kind, id.to_owned());
        if let Some(number) = self.numbers.get(&key) {
            return *number;
        }
        let number = self.issued.keys().next_back().map_or(1, |last| last + 1);
        self.issued.insert(number, (kind, Some(id.to_owned())));
        self.numbers.insert(key, number);
        number
    }

    fn version(&self, board: &str) -> String {
        self.versions
            .get(board)
            .cloned()
            .unwrap_or_else(|| "0".to_owned())
    }

    /// `number` on `id`, issued elsewhere.
    fn hold_number(&mut self, kind: Kind, id: &str, number: u64) {
        self.numbers.insert((kind, id.to_owned()), number);
        self.issued.insert(number, (kind, Some(id.to_owned())));
    }

    fn retire(&mut self, kind: Kind, id: &str) {
        if let Some(number) = self.numbers.remove(&(kind, id.to_owned()))
            && let Some(entry) = self.issued.get_mut(&number)
        {
            entry.1 = None;
        }
    }

    fn hold_asset(&mut self, owner: Owner, name: &str, bytes: &[u8]) {
        self.assets
            .entry(owner)
            .or_default()
            .insert(name.to_owned(), bytes.to_vec());
    }

    /// Whether what `owner` names is here.
    fn holds(&self, owner: &Owner) -> super::Result<()> {
        match owner {
            Owner::Article(id) if !self.articles.contains_key(id) => Err(not_found("article", id)),
            Owner::Page { board, .. } if !self.boards.contains_key(board) => {
                Err(not_found("board", board))
            }
            _ => Ok(()),
        }
    }

    /// Drop the pictures of every page of `board` that `keep` turns away.
    fn drop_pages(&mut self, board: &str, keep: impl Fn(&Page) -> bool) {
        self.assets.retain(|owner, _| match owner {
            Owner::Page { board: on, page } => on != board || keep(page),
            _ => true,
        });
    }

    fn article(&mut self, id: &str) -> super::Result<&mut Held> {
        self.articles
            .get_mut(id)
            .ok_or_else(|| not_found("article", id))
    }

    /// An article as it is held, its number issued if it has none.
    fn describe(&mut self, id: &str) -> Option<Article> {
        let number = self.number(Kind::Article, id);
        let held = self.articles.get(id)?;
        let properties = properties::parse(&held.properties);
        Some(Article {
            id: id.to_owned(),
            number: Some(number),
            title: properties.title,
            archived: properties.archived,
            touched: held.touched,
            created: stamp::named(id),
            cover: None,
            labels: properties.labels,
        })
    }
}

impl super::Project for Project {
    fn boards(&self) -> Vec<Board> {
        let mut state = self.state();
        let ids: Vec<String> = state.boards.keys().cloned().collect();
        let mut boards: Vec<Board> = ids
            .into_iter()
            .map(|id| {
                let number = state.number(Kind::Board, &id);
                let mut board = state.boards[&id].clone();
                board.number = Some(number);
                board.version = Some(state.version(&id));
                board
            })
            .collect();
        boards.sort_by_key(|board| Reverse(board.touched));
        boards
    }

    fn board(&self, id: &str) -> Option<Board> {
        let mut state = self.state();
        let mut board = state.boards.get(id)?.clone();
        board.number = Some(state.number(Kind::Board, id));
        board.version = Some(state.version(id));
        Some(board)
    }

    fn create_board(
        &self,
        name: &str,
        key: &str,
    ) -> impl Future<Output = super::Result<Board>> + Send + use<> {
        let mut state = self.state();
        let mut board = Board::new(id::mint(), name);
        board.created = stamp::named(&board.id);
        board.key = key::or_derived(key, &board.name, || {
            state
                .boards
                .values()
                .map(|board| board.key.clone())
                .collect()
        });
        board.touched = stamp::now();
        board.number = Some(state.number(Kind::Board, &board.id));
        board.version = Some(state.version(&board.id));
        state.boards.insert(board.id.clone(), board.clone());
        ready(Ok(board))
    }

    fn save_board(
        &self,
        board: &mut Board,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        if let Some(seen) = &board.version
            && (!state.boards.contains_key(&board.id) || *seen != state.version(&board.id))
        {
            return ready(Err(crate::error::Error::Stale));
        }
        let kept: HashSet<&str> = board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .map(|card| card.id.as_str())
            .collect();
        let gone: HashSet<String> = state
            .boards
            .get(&board.id)
            .into_iter()
            .flat_map(|held| held.columns.iter().flat_map(|column| &column.cards))
            .map(|card| card.id.clone())
            .filter(|id| !kept.contains(id.as_str()))
            .collect();
        state.drop_pages(&board.id, |page| match page {
            Page::Card(card) => !gone.contains(card),
            Page::Description => true,
        });
        state.saves += 1;
        let version = state.saves.to_string();
        state.versions.insert(board.id.clone(), version.clone());
        board.touched = stamp::now();
        board.version = Some(version);
        state.boards.insert(board.id.clone(), board.clone());
        ready(Ok(()))
    }

    fn remove_board(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        if state.boards.remove(id).is_none() {
            return ready(Err(not_found("board", id)));
        }
        state.drop_pages(id, |_| false);
        state.retire(Kind::Board, id);
        ready(Ok(()))
    }

    fn sessions(&self) -> Vec<Record> {
        let mut state = self.state();
        let ids: Vec<String> = state.sessions.keys().cloned().collect();
        let mut found: Vec<Record> = ids
            .into_iter()
            .map(|id| {
                let number = state.number(Kind::Session, &id);
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
        record.number = Some(state.number(Kind::Session, id));
        Some(record)
    }

    fn create_session(&self) -> impl Future<Output = super::Result<String>> + Send + use<> {
        ready(Ok(stamp::fresh().to_string()))
    }

    fn save_session(
        &self,
        record: &Record,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.state()
            .sessions
            .insert(record.id.clone(), record.clone());
        ready(Ok(()))
    }

    fn remove_session(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        if state.sessions.remove(id).is_none() {
            return ready(Err(not_found("session", id)));
        }
        state.retire(Kind::Session, id);
        ready(Ok(()))
    }

    fn articles(&self) -> Vec<Article> {
        let mut state = self.state();
        let ids: Vec<String> = state.articles.keys().cloned().collect();
        let mut found: Vec<Article> = ids.iter().filter_map(|id| state.describe(id)).collect();
        found.sort_by_key(|article| Reverse(article.touched));
        found
    }

    fn article(&self, id: &str) -> Option<Article> {
        self.state().describe(id)
    }

    fn create_article(
        &self,
        markdown: &str,
    ) -> impl Future<Output = super::Result<Article>> + Send + use<> {
        let mut state = self.state();
        let id = stamp::free(stamp::now(), |id| state.articles.contains_key(id));
        let held = Held {
            markdown: markdown.to_owned(),
            touched: stamp::now(),
            ..Held::default()
        };
        state.articles.insert(id.clone(), held);
        ready(state.describe(&id).ok_or_else(|| not_found("article", &id)))
    }

    fn read_article(&self, id: &str) -> super::Result<String> {
        Ok(self.state().article(id)?.markdown.clone())
    }

    fn write_article(
        &self,
        id: &str,
        markdown: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        ready(state.article(id).map(|held| {
            held.comments.remap(&held.markdown, markdown);
            held.markdown = markdown.to_owned();
            held.touched = stamp::now();
        }))
    }

    fn comments(&self, id: &str) -> Comments {
        let mut state = self.state();
        state
            .article(id)
            .map(|held| held.comments.clone())
            .unwrap_or_default()
    }

    fn save_comments(
        &self,
        id: &str,
        comments: &Comments,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        ready(state.article(id).map(|held| {
            held.comments = comments.clone();
            held.comments.stamp(&held.markdown);
        }))
    }

    fn properties(&self, id: &str) -> Properties {
        self.state()
            .articles
            .get(id)
            .map(|held| properties::parse(&held.properties))
            .unwrap_or_default()
    }

    fn save_properties(
        &self,
        id: &str,
        properties: &Properties,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        ready(state.article(id).map(|held| {
            held.properties = properties::apply(&held.properties, properties).unwrap_or_default();
            held.touched = stamp::now();
        }))
    }

    fn remove_article(&self, id: &str) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        if state.articles.remove(id).is_none() {
            return ready(Err(not_found("article", id)));
        }
        state.assets.remove(&Owner::Article(id.to_owned()));
        state.retire(Kind::Article, id);
        ready(Ok(()))
    }

    fn asset(
        &self,
        owner: &Owner,
        name: &str,
    ) -> impl Future<Output = super::Result<Vec<u8>>> + Send + use<> {
        let state = self.state();
        ready(state.holds(owner).and_then(|()| {
            state
                .assets
                .get(owner)
                .and_then(|held| held.get(name))
                .cloned()
                .ok_or_else(|| not_found("asset", name))
        }))
    }

    fn put_asset(
        &self,
        owner: &Owner,
        name: &str,
        bytes: &[u8],
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let mut state = self.state();
        let put = component(name)
            .map_err(Error::from)
            .and_then(|name| state.holds(owner).map(|()| name));
        ready(put.map(|name| state.hold_asset(owner.clone(), name, bytes)))
    }

    /// A card's pictures are its files here.
    fn carry_card_files(
        &self,
        board: &str,
        card: &str,
        to: &Self,
        to_board: &str,
        to_card: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        let from = Owner::Page {
            board: board.to_owned(),
            page: Page::Card(card.to_owned()),
        };
        let held = self.state().assets.get(&from).cloned();
        if let Some(held) = held {
            let landing = Owner::Page {
                board: to_board.to_owned(),
                page: Page::Card(to_card.to_owned()),
            };
            to.state().assets.entry(landing).or_default().extend(held);
        }
        ready(Ok(()))
    }

    fn number(
        &self,
        kind: Kind,
        id: &str,
    ) -> impl Future<Output = super::Result<u64>> + Send + use<> {
        ready(Ok(self.state().number(kind, id)))
    }

    fn resolve(&self, kind: Kind, number: u64) -> super::Result<Option<String>> {
        Ok(self
            .state()
            .issued
            .get(&number)
            .filter(|(issued, _)| *issued == kind)
            .and_then(|(_, id)| id.clone()))
    }

    fn retire(
        &self,
        kind: Kind,
        id: &str,
    ) -> impl Future<Output = super::Result<()>> + Send + use<> {
        self.state().retire(kind, id);
        ready(Ok(()))
    }
}

fn not_found(what: &'static str, named: &str) -> Error {
    Error::NotFound {
        what,
        named: named.to_owned(),
    }
}
