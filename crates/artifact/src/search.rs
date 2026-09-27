//! Finding text across a project's articles, boards and sessions.
//!
//! No index: a search walks the text it is handed. [`run`] searches what a
//! caller already holds in memory; [`disk`] reads a project off its
//! `.cydonia/`, open in the app or not.
//!
//! Matches stream over a channel as they are found, in no particular order,
//! and a search stops early once the receiver is dropped.

use crate::{
    article::Article,
    board::Board,
    project::fs,
    session::{chat::ChatItem, record::Record},
};
use regex::bytes::{Regex, RegexBuilder};
use std::{ops::Range, sync::mpsc::Sender};

/// What a search looks for: a literal, case-insensitive.
#[derive(Clone)]
pub struct Query {
    text: String,
    pattern: Regex,
}

impl Query {
    /// `None` for a query that is blank once trimmed.
    pub fn literal(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(Self {
            text: text.to_owned(),
            pattern: literal(text),
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether `bytes` hold a match anywhere.
    pub fn is_match(&self, bytes: &[u8]) -> bool {
        self.pattern.is_match(bytes)
    }

    /// Every match in `text`, as byte ranges into it.
    pub fn find<'a>(&'a self, text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.pattern.find_iter(text.as_bytes()).map(|m| m.range())
    }
}

fn literal(text: &str) -> Regex {
    RegexBuilder::new(&regex::escape(text))
        .case_insensitive(true)
        .build()
        .expect("an escaped literal is a valid pattern")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Article,
    Board,
    Session,
}

impl Kind {
    /// The word entry numbers are issued under — see
    /// [`crate::project::Project::number`].
    pub fn key(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Board => "board",
            Self::Session => "session",
        }
    }
}

/// The artifact a match is in.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Item {
    pub kind: Kind,
    pub id: String,
}

/// Where in its item a match is. Each names a run of text the item owns, and
/// a match's range is a byte range into that text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Block {
    /// The item's title: an article's title, a board's name, a session's name
    /// or else its title.
    Title,
    /// A line of an article's markdown, counted from 0.
    Line(usize),
    /// A card's text, by the card's id.
    Card(String),
    /// A transcript item, by its index in [`Record::items`].
    Chat(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub item: Item,
    pub block: Block,
    /// Bytes into the block's text.
    pub range: Range<usize>,
    /// The line of the block holding the start of the match.
    pub line: String,
    /// Bytes into [`Match::line`].
    pub column: Range<usize>,
}

/// Something a search walks: an item and its blocks of text.
pub trait Searchable {
    fn item(&self) -> Item;

    /// Hand each block of text to `each`, in order.
    fn blocks(&self, each: &mut dyn FnMut(Block, &str));
}

/// An article with its markdown, which [`Article`] does not carry.
pub struct Document<'a> {
    pub article: &'a Article,
    pub markdown: &'a str,
}

impl Searchable for Document<'_> {
    fn item(&self) -> Item {
        Item {
            kind: Kind::Article,
            id: self.article.id.clone(),
        }
    }

    fn blocks(&self, each: &mut dyn FnMut(Block, &str)) {
        each(Block::Title, &self.article.title);
        for (ix, line) in self.markdown.lines().enumerate() {
            each(Block::Line(ix), line);
        }
    }
}

impl Searchable for Board {
    fn item(&self) -> Item {
        Item {
            kind: Kind::Board,
            id: self.id.clone(),
        }
    }

    fn blocks(&self, each: &mut dyn FnMut(Block, &str)) {
        each(Block::Title, &self.name);
        for card in self.columns.iter().flat_map(|column| &column.cards) {
            each(Block::Card(card.id.clone()), &card.text);
        }
    }
}

impl Searchable for Record {
    fn item(&self) -> Item {
        Item {
            kind: Kind::Session,
            id: self.id.clone(),
        }
    }

    fn blocks(&self, each: &mut dyn FnMut(Block, &str)) {
        each(Block::Title, self.name.as_deref().unwrap_or(&self.title));
        for (ix, item) in self.items.iter().enumerate() {
            if let Some(text) = searchable(item) {
                each(Block::Chat(ix), text);
            }
        }
    }
}

/// The text of a transcript item a search looks through.
pub fn searchable(item: &ChatItem) -> Option<&str> {
    match item {
        ChatItem::User(text) | ChatItem::Agent(text) => Some(text),
        ChatItem::Tool { label, .. } => Some(label),
        ChatItem::Notice { text, .. } => Some(text),
        ChatItem::Thinking { .. } | ChatItem::Process { .. } => None,
    }
}

/// Every match of `query` in one item, sent to `found`. `false` once the
/// receiver is gone.
pub fn one(source: &dyn Searchable, query: &Query, found: &Sender<Match>) -> bool {
    let item = source.item();
    let mut open = true;
    source.blocks(&mut |block, text| {
        if !open || !query.is_match(text.as_bytes()) {
            return;
        }
        for range in query.find(text) {
            let start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
            let end = text[range.start..]
                .find('\n')
                .map_or(text.len(), |at| range.start + at);
            let hit = Match {
                item: item.clone(),
                block: block.clone(),
                line: text[start..end].to_owned(),
                column: range.start - start..range.end.min(end) - start,
                range,
            };
            if found.send(hit).is_err() {
                open = false;
                return;
            }
        }
    });
    open
}

/// Search every source, spread over the machine's threads.
pub fn run(sources: &[&(dyn Searchable + Sync)], query: &Query, found: &Sender<Match>) {
    parallel(sources, |source| one(*source, query, found));
}

/// Search a project off its `.cydonia/`. An item's raw file is checked for
/// the query before it is parsed, so a file holding no match costs one read.
///
/// Reads only: nothing a read of the project would write back is written.
pub fn disk(project: &fs::Project, query: &Query, found: &Sender<Match>) {
    // Sessions are JSON, so the query is looked for as JSON writes it.
    let escaped = serde_json::to_string(query.text()).unwrap_or_default();
    let in_json = literal(&escaped[1..escaped.len() - 1]);
    let mut files: Vec<(Kind, String, std::path::PathBuf)> = Vec::new();
    files.extend(
        project
            .session_files()
            .into_iter()
            .map(|(id, path)| (Kind::Session, id, path)),
    );
    files.extend(
        project
            .article_files()
            .into_iter()
            .map(|(id, path)| (Kind::Article, id, path)),
    );
    files.extend(
        project
            .board_files()
            .into_iter()
            .map(|(id, path)| (Kind::Board, id, path)),
    );
    parallel(&files, |(kind, id, path)| {
        let Ok(bytes) = std::fs::read(path) else {
            return true;
        };
        match kind {
            Kind::Session => {
                if !in_json.is_match(&bytes) {
                    return true;
                }
                let Ok(mut record) = serde_json::from_slice::<Record>(&bytes) else {
                    return true;
                };
                if record.id.is_empty() {
                    record.id = id.clone();
                }
                one(&record, query, found)
            }
            Kind::Article => {
                let Some(article) = project.describe_article(path) else {
                    return true;
                };
                let markdown = String::from_utf8_lossy(&bytes);
                if !query.is_match(&bytes) && !query.is_match(article.title.as_bytes()) {
                    return true;
                }
                one(
                    &Document {
                        article: &article,
                        markdown: &markdown,
                    },
                    query,
                    found,
                )
            }
            Kind::Board => {
                // TOML escapes too, but a board is small enough to parse.
                let Some(board) = project.read_board_file(path) else {
                    return true;
                };
                one(&board, query, found)
            }
        }
    });
}

/// Call `each` on every item, over the machine's threads, until one says
/// `false`.
#[cfg(not(target_family = "wasm"))]
fn parallel<T: Sync>(items: &[T], each: impl Fn(&T) -> bool + Sync) {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items.len());
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) else {
                        break;
                    };
                    if !each(item) {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            });
        }
    });
}

#[cfg(target_family = "wasm")]
fn parallel<T: Sync>(items: &[T], each: impl Fn(&T) -> bool + Sync) {
    for item in items {
        if !each(item) {
            break;
        }
    }
}
