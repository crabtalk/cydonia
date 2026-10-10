//! What a written reference, `[project]#ref[:from[-to] | #anchor]`, names
//! among the open projects — see [`artifact::reference`].

use super::{Showing, Workspace};
use crate::model::{article::Disk, project::Project};
use artifact::{
    article::anchor,
    reference::{self, Span, Target, Within},
    search::Kind,
};
use bezel::gpui::{App, Entity, Global, WeakEntity};
use std::{ops::Range, path::PathBuf, rc::Rc};

/// An entry a reference resolved to.
pub struct Resolved {
    pub project: PathBuf,
    pub showing: Showing,
    pub kind: Kind,
    pub number: u64,
    pub part: Option<Part>,
    /// The entry's own title, empty where it has none.
    pub title: String,
}

/// The part of an entry a reference names after its number.
#[derive(Clone)]
pub enum Part {
    /// A run of a session's turns.
    Turns(Span),
    /// One of a board's cards.
    Card(Card),
    /// The blocks of an article a run of its lines or a heading's section
    /// covers.
    Passage(Passage),
}

#[derive(Clone)]
pub struct Passage {
    /// The blocks, numbered as the article's editor numbers them.
    pub blocks: Range<usize>,
    /// The bytes of [`Disk::text`] the blocks were parsed from.
    pub source: Range<usize>,
    pub disk: Rc<Disk>,
    /// What the reference named: `lines 5-7`, or the heading's text.
    pub label: String,
}

#[derive(Clone)]
pub struct Card {
    pub id: String,
    /// `DEV-12`, as the board writes it.
    pub handle: String,
    pub text: String,
}

impl Passage {
    pub fn markdown(&self) -> &str {
        &self.disk.text[self.source.clone()]
    }
}

/// The workspace, for readers handed only `&App` — a link preview.
struct Current(WeakEntity<Workspace>);

impl Global for Current {}

impl Workspace {
    /// Make this workspace the one [`resolve_in`] reads.
    pub fn install(this: &Entity<Self>, cx: &mut App) {
        cx.set_global(Current(this.downgrade()));
    }

    /// The entry a reference names, or why it names none. A reference with
    /// no project is read in the active one.
    pub fn resolve(&self, text: &str) -> Result<Resolved, String> {
        let Some(reference) = reference::parse(text) else {
            return Err(format!("{text} is not a reference"));
        };
        let Some(project) = self.named_project(reference.project) else {
            return Err(format!("No open project for {text}"));
        };
        let (number, within) = match reference.target {
            Target::Entry { number, within } => (number, within),
            Target::Card { key, handle } => return card(project, key, handle, text),
        };
        let Some((kind, showing, title)) = entry(project, number) else {
            return Err(format!("Nothing is {text}"));
        };
        let part = match (within, &showing) {
            (None, _) => None,
            (Some(Within::Span(turns)), Showing::Session(_)) => Some(Part::Turns(turns)),
            (Some(within), Showing::Article(id)) => {
                let article = project.articles.iter().find(|article| &article.id == id);
                let disk = article.map(|article| article.disk()).unwrap_or_default();
                Some(Part::Passage(passage(disk, within, text)?))
            }
            (Some(Within::Heading(_)), Showing::Session(_)) => {
                return Err(format!("{text} names a heading, which only an article has"));
            }
            (Some(_), _) => return Err(format!("{text} names part of an entry that has none")),
        };
        Ok(Resolved {
            project: project.path.clone(),
            showing,
            kind,
            number,
            part,
            title,
        })
    }

    fn named_project(&self, name: Option<&str>) -> Option<&Project> {
        match name {
            Some(name) => self
                .projects
                .iter()
                .find(|project| project.path.file_name().is_some_and(|last| last == name)),
            None => self.active_project(),
        }
    }
}

/// The entry `number` names in `project`: its kind, what shows it, and its
/// own title.
fn entry(project: &Project, number: u64) -> Option<(Kind, Showing, String)> {
    project
        .sessions
        .iter()
        .find(|chat| chat.number == Some(number))
        .map(|chat| (Kind::Session, Showing::Session(chat.id), chat.title.clone()))
        .or_else(|| {
            project
                .articles
                .iter()
                .find(|article| article.number == Some(number))
                .map(|article| {
                    (
                        Kind::Article,
                        Showing::Article(article.id.clone()),
                        article.title.clone(),
                    )
                })
        })
        .or_else(|| {
            project
                .boards
                .iter()
                .find(|board| board.number == Some(number))
                .map(|board| {
                    (
                        Kind::Board,
                        Showing::Board(board.id.clone()),
                        board.name.clone(),
                    )
                })
        })
        .or_else(|| {
            project
                .tables
                .iter()
                .find(|table| table.number == Some(number))
                .map(|table| {
                    (
                        Kind::Table,
                        Showing::Table(table.key.clone()),
                        table.name.clone(),
                    )
                })
        })
}

/// The card `key-handle` names in `project`, as a part of its board. `text`
/// is the reference, for the refusal.
fn card(project: &Project, key: &str, handle: u64, text: &str) -> Result<Resolved, String> {
    let Some(board) = project
        .boards
        .iter()
        .find(|board| board.key.eq_ignore_ascii_case(key))
    else {
        return Err(format!("No board has the key {key}"));
    };
    let Some(found) = board
        .columns
        .iter()
        .flat_map(|column| column.cards.iter())
        .find(|card| card.handle == Some(handle))
    else {
        return Err(format!("Nothing is {text}"));
    };
    let Some(number) = board.number else {
        return Err(format!("{text} is on a board with no number"));
    };
    Ok(Resolved {
        project: project.path.clone(),
        showing: Showing::Board(board.id.clone()),
        kind: Kind::Board,
        number,
        part: Some(Part::Card(Card {
            id: found.id.clone(),
            handle: board
                .handle_of(found)
                .unwrap_or_else(|| format!("{key}-{handle}")),
            text: found.text.clone(),
        })),
        title: board.name.clone(),
    })
}

/// The blocks of `disk` a span of lines or a heading names. `text` is the
/// reference, for the refusal.
fn passage(disk: Rc<Disk>, within: Within<'_>, text: &str) -> Result<Passage, String> {
    let (bytes, label) = match within {
        Within::Span(span) => {
            let bytes = anchor::lines(&disk.text, span).ok_or_else(|| {
                format!(
                    "{text} starts past the article's last line, {}",
                    disk.text.lines().count()
                )
            })?;
            let label = match span.from == span.to {
                true => format!("line {span}"),
                false => format!("lines {span}"),
            };
            (bytes, label)
        }
        Within::Heading(name) => {
            let (heading, bytes) = anchor::section(&disk.text, name)
                .ok_or_else(|| format!("{text} names a heading the article does not have"))?;
            (bytes, heading.text)
        }
    };
    let first = disk
        .blocks
        .iter()
        .position(|block| !block.is_empty() && block.end > bytes.start);
    let last = disk
        .blocks
        .iter()
        .rposition(|block| !block.is_empty() && block.start < bytes.end);
    let (Some(first), Some(last)) = (first, last) else {
        return Err(format!("{text} names nothing in the article"));
    };
    Ok(Passage {
        blocks: first..last + 1,
        source: disk.blocks[first].start..disk.blocks[last].end,
        disk,
        label,
    })
}

/// [`Workspace::resolve`] on the installed workspace.
pub fn resolve_in(text: &str, cx: &App) -> Option<Resolved> {
    let workspace = cx.try_global::<Current>()?.0.upgrade()?;
    workspace.read(cx).resolve(text).ok()
}

#[cfg(test)]
#[path = "../../../tests/unit/references.rs"]
mod tests;
