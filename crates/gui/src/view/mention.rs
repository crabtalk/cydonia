//! What `@` in an article lists: the open projects' sessions, boards and
//! articles, linked as `cydonia://<project>#<number>` chips — and the title a
//! chip of one paints: cydonia's mark and the title.

use crate::{
    model::workspace::{Showing, Workspace, references::Part},
    view::{entry_link::link, search::kind_icon, sidebar::Row},
};
use artifact::{
    reference::{self, Prefix},
    search::Kind,
};
use bezel::gpui::{App, Global, SharedString};
use bezel::ui::icons::Icon;
use editor::Mention;
use markdown::Preview;

/// Rows the menu lists at most.
const SHOWN: usize = 20;

/// The mark every `cydonia://` chip paints. Inline chips paint only a glyph's
/// SVG, in the text colour — a file icon paints nothing there.
const MARK: Icon = Icon::glyph(include_bytes!("../../assets/mark.svg"));

#[derive(Clone)]
pub(crate) struct Linkable {
    /// A session's agent's mark, else its kind's.
    pub(crate) icon: Icon,
    pub(crate) title: String,
    pub(crate) about: String,
    pub(crate) url: String,
    pub(crate) touched: u128,
    /// The directory name of the project it is in.
    pub(crate) project: String,
    /// In the active project, the one a bare `#12` means.
    pub(crate) active: bool,
    pub(crate) number: u64,
    /// A board's card prefix.
    pub(crate) key: Option<String>,
    /// A session's agent.
    pub(crate) agent: Option<String>,
    pub(crate) archived: bool,
    pub(crate) kind: Kind,
    /// The sidebar's row for it.
    pub(crate) row: Row,
}

/// The entries `@` can link, most recently touched first. Rebuilt whenever
/// the workspace changes.
#[derive(Default)]
pub(crate) struct Linkables(pub(crate) Vec<Linkable>);

impl Global for Linkables {}

/// Every open project's entries, as what `@` lists. Installed with
/// `cx.set_global`.
pub(crate) fn read(workspace: &Workspace) -> Linkables {
    let active = workspace.active_project().map(|project| &project.path);
    let mut held = Vec::new();
    for project in &workspace.projects {
        let Some(name) = project.path.file_name().map(|name| name.to_string_lossy()) else {
            continue;
        };
        let active = active == Some(&project.path);
        let reference = |number: u64| format!("{name}#{number}");
        let shown = |number: u64| match active {
            true => format!("#{number}"),
            false => reference(number),
        };
        let row = |showing: Showing| Row::Entry {
            project: project.path.clone(),
            showing,
        };
        let linkable =
            |icon: Icon, title: String, about: String, number: u64, touched: u128, row: Row| {
                Linkable {
                    archived: false,
                    kind: Kind::Session,
                    icon,
                    title,
                    about,
                    url: link(&reference(number)),
                    touched,
                    project: name.to_string(),
                    active,
                    number,
                    key: None,
                    agent: None,
                    row,
                }
            };
        held.extend(project.sessions.iter().filter_map(|chat| {
            let number = chat.number?;
            Some(Linkable {
                agent: Some(chat.entry.name.clone()),
                archived: chat.closed,
                ..linkable(
                    workspace
                        .agent_icon(&chat.entry.name)
                        .unwrap_or_else(|| kind_icon(Kind::Session)),
                    untitled(&chat.title, "Untitled session"),
                    format!("Session · {} · {}", chat.entry.name, shown(number)),
                    number,
                    chat.touched(),
                    row(Showing::Session(chat.id)),
                )
            })
        }));
        held.extend(project.articles.iter().filter_map(|article| {
            let number = article.number?;
            Some(Linkable {
                archived: article.archived,
                kind: Kind::Article,
                ..linkable(
                    kind_icon(Kind::Article),
                    untitled(&article.title, "Untitled article"),
                    format!("Article · {}", shown(number)),
                    number,
                    article.touched,
                    row(Showing::Article(article.id.clone())),
                )
            })
        }));
        held.extend(project.boards.iter().filter_map(|board| {
            let number = board.number?;
            let key = (!board.key.is_empty()).then(|| board.key.clone());
            let about = match &key {
                Some(key) => format!("Board · {key} · {}", shown(number)),
                None => format!("Board · {}", shown(number)),
            };
            Some(Linkable {
                key,
                archived: board.archived,
                kind: Kind::Board,
                ..linkable(
                    kind_icon(Kind::Board),
                    untitled(&board.name, "Untitled board"),
                    about,
                    number,
                    board.touched,
                    row(Showing::Board(board.id.clone())),
                )
            })
        }));
    }
    held.sort_by_key(|linkable| std::cmp::Reverse(linkable.touched));
    Linkables(held)
}

fn untitled(title: &str, fallback: &str) -> String {
    match title.trim() {
        "" => fallback.to_owned(),
        title => title.to_owned(),
    }
}

/// The mention source cydonia installs — see [`rank`].
pub(crate) fn source(query: &str, cx: &App) -> Vec<Mention> {
    let Some(Linkables(held)) = cx.try_global::<Linkables>() else {
        return Vec::new();
    };
    rank(query, held)
        .into_iter()
        .take(SHOWN)
        .map(|ix| {
            let linkable = &held[ix];
            Mention {
                icon: Some(linkable.icon.clone()),
                label: SharedString::from(linkable.title.clone()),
                description: Some(SharedString::from(linkable.about.clone())),
                url: linkable.url.clone(),
            }
        })
        .collect()
}

/// What `query` lists, as positions in `held`, best first. An empty query
/// lists the active project, archived entries last. A query opening with a
/// kind's prefix — see [`kind_prefix`] — lists that kind alone.
///
/// Each entry is placed by its best match: an exact reference — see
/// [`references`] — then a title starting with the query, then a reference
/// prefix or a title holding it, then a session's agent holding it. Titles
/// are matched in every project, and not for a query holding `#`. Archived
/// entries follow every other match but an exact reference. Within a place
/// the active project leads; ties keep the most recently touched first.
pub(crate) fn rank(query: &str, held: &[Linkable]) -> Vec<usize> {
    let (kind, query) = kind_prefix(query);
    let mut rows = rank_all(query, held);
    if let Some(kind) = kind {
        rows.retain(|&ix| held[ix].kind == kind);
    }
    rows
}

/// The kind a query opens with, `s:`, `a:` or `b:` in either case, and the
/// query after it.
pub(crate) fn kind_prefix(query: &str) -> (Option<Kind>, &str) {
    let query = query.trim_start();
    let kind = match query.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("s:") => Kind::Session,
        Some("a:") => Kind::Article,
        Some("b:") => Kind::Board,
        _ => return (None, query),
    };
    (Some(kind), &query[2..])
}

fn rank_all(query: &str, held: &[Linkable]) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        let mut rows: Vec<usize> = (0..held.len()).filter(|&ix| held[ix].active).collect();
        rows.sort_by_key(|&ix| held[ix].archived);
        return rows;
    }
    let mut place: Vec<Option<u8>> = vec![None; held.len()];
    for ix in references(query, held) {
        place[ix] = Some(match exact(query, &held[ix]) {
            true => 0,
            false => 2,
        });
    }
    if !query.contains('#') {
        let lower = query.to_lowercase();
        for (ix, linkable) in held.iter().enumerate() {
            let title = linkable.title.to_lowercase();
            let found = if title.starts_with(&lower) {
                Some(1)
            } else if title.contains(&lower) {
                Some(2)
            } else if linkable
                .agent
                .as_ref()
                .is_some_and(|agent| agent.to_lowercase().contains(&lower))
            {
                Some(3)
            } else {
                None
            };
            place[ix] = match (place[ix], found) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
    }
    let mut rows: Vec<(bool, u8, bool, usize)> = place
        .into_iter()
        .enumerate()
        .filter_map(|(ix, place)| {
            let place = place?;
            let sunk = place > 0 && held[ix].archived;
            Some((sunk, place, !held[ix].active, ix))
        })
        .collect();
    rows.sort_by_key(|&(sunk, place, inactive, _)| (sunk, place, inactive));
    rows.into_iter().map(|(.., ix)| ix).collect()
}

/// Whether `query` names `linkable` exactly: its number or its key.
fn exact(query: &str, linkable: &Linkable) -> bool {
    let target = query.rsplit_once('#').map_or(query, |(_, rest)| rest);
    target == linkable.number.to_string()
        || linkable
            .key
            .as_ref()
            .is_some_and(|key| key.eq_ignore_ascii_case(target))
}

/// The entries `query` starts a reference to, as positions in `held`, best
/// first. Nothing for an empty query.
///
/// Read as [`reference::partial`]: a number prefix lists the entries whose
/// number starts with it, a key prefix the boards whose key does, each in the
/// project named before the `#` or the active one. An exact number or key
/// leads; ties keep the most recently touched first.
fn references(query: &str, held: &[Linkable]) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let Some(partial) = reference::partial(query) else {
        return Vec::new();
    };
    let scoped: Vec<usize> = (0..held.len())
        .filter(|&ix| match partial.project {
            None => held[ix].active,
            Some(name) => held[ix].project.eq_ignore_ascii_case(name),
        })
        .collect();
    match partial.target {
        Prefix::Number(number) => by_prefix(&scoped, number, held, |linkable| {
            Some(linkable.number.to_string())
        }),
        Prefix::Key(key) => by_prefix(&scoped, key, held, |linkable| {
            linkable.key.as_ref().map(|key| key.to_lowercase())
        }),
    }
}

/// The positions in `scoped` whose `field` starts with `prefix`, compared
/// lowercase, the exact one first.
fn by_prefix(
    scoped: &[usize],
    prefix: &str,
    held: &[Linkable],
    field: impl Fn(&Linkable) -> Option<String>,
) -> Vec<usize> {
    let prefix = prefix.to_lowercase();
    let mut rows: Vec<(bool, usize)> = scoped
        .iter()
        .filter_map(|&ix| {
            let value = field(&held[ix])?;
            value.starts_with(&prefix).then_some((value != prefix, ix))
        })
        .collect();
    rows.sort_by_key(|&(inexact, _)| inexact);
    rows.into_iter().map(|(_, ix)| ix).collect()
}

/// What a chip linking an entry paints: its title, and its kind's mark. A
/// run of a session's turns adds the run, and part of an article the lines or
/// the heading. A card paints its own title and handle. A link naming no entry
/// paints the reference and the app's mark.
pub(crate) fn preview(url: &str, cx: &App) -> Option<Preview> {
    let reference = url.strip_prefix(crate::view::entry_link::SCHEME)?;
    let Some(resolved) = crate::model::workspace::references::resolve_in(reference, cx) else {
        return Some(Preview {
            title: Some(SharedString::from(reference.to_owned())),
            glyph: Some(MARK),
            ..Preview::default()
        });
    };
    let fallback = match resolved.kind {
        Kind::Session => "Untitled session",
        Kind::Article => "Untitled article",
        Kind::Board => "Untitled board",
        Kind::Table => "Untitled table",
    };
    let title = untitled(&resolved.title, fallback);
    let title = match resolved.part {
        Some(Part::Turns(turns)) if turns.from == turns.to => format!("{title} · turn {turns}"),
        Some(Part::Turns(turns)) => format!("{title} · turns {turns}"),
        Some(Part::Passage(passage)) => format!("{title} · {}", passage.label),
        Some(Part::Card(card)) => format!(
            "{} · {}",
            crate::view::board::card_title(&card.text),
            card.handle
        ),
        None => title,
    };
    Some(Preview {
        title: Some(title.into()),
        glyph: Some(MARK),
        ..Preview::default()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/mention_rank.rs"]
mod rank_tests;
