//! What `@` in an article lists: the open projects' sessions, boards and
//! articles, linked as `cydonia://<project>#<number>` chips — and the title a
//! chip of one paints: the kind's mark and the title.

use crate::{model::workspace::Workspace, view::entry_link::link};
use artifact::search::Kind;
use bezel::{
    gpui::{App, Global, SharedString},
    ui::popover::filter_indices,
};
use editor::Mention;
use markdown::Preview;

/// Rows the menu lists at most.
const SHOWN: usize = 20;

#[derive(Clone)]
pub(crate) struct Linkable {
    pub(crate) kind: Kind,
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
        let linkable =
            |kind: Kind, title: String, about: String, number: u64, touched: u128| Linkable {
                kind,
                title,
                about,
                url: link(&reference(number)),
                touched,
                project: name.to_string(),
                active,
                number,
                key: None,
            };
        held.extend(project.sessions.iter().filter_map(|chat| {
            let number = chat.number?;
            Some(linkable(
                Kind::Session,
                untitled(&chat.title, "Untitled session"),
                format!("Session · {} · {}", chat.entry.name, shown(number)),
                number,
                chat.touched(),
            ))
        }));
        held.extend(project.articles.iter().filter_map(|article| {
            let number = article.number?;
            Some(linkable(
                Kind::Article,
                untitled(&article.title, "Untitled article"),
                format!("Article · {}", shown(number)),
                number,
                article.touched,
            ))
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
                ..linkable(
                    Kind::Board,
                    untitled(&board.name, "Untitled board"),
                    about,
                    number,
                    board.touched,
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
                icon: Some(crate::view::search::kind_icon(linkable.kind)),
                label: SharedString::from(linkable.title.clone()),
                description: Some(SharedString::from(linkable.about.clone())),
                url: linkable.url.clone(),
            }
        })
        .collect()
}

/// What `query` lists, as positions in `held`, best first.
///
/// A query with a `#` is a reference: `#12` and `bezel#12` list the entries
/// of the active project and of `bezel` whose number starts with what follows
/// the `#`. Without one, it lists the active project's entries: those whose
/// number starts with an all-digit query, then boards whose key starts with
/// it, then the rest by title. An exact number or key leads its group; ties
/// keep the most recently touched first.
pub(crate) fn rank(query: &str, held: &[Linkable]) -> Vec<usize> {
    let query = query.trim();
    if let Some((project, number)) = query.split_once('#') {
        if !number.bytes().all(|b| b.is_ascii_digit()) {
            return Vec::new();
        }
        let scoped: Vec<usize> = (0..held.len())
            .filter(|&ix| match project {
                "" => held[ix].active,
                name => held[ix].project.eq_ignore_ascii_case(name),
            })
            .collect();
        return by_number(&scoped, number, held);
    }
    let active: Vec<usize> = (0..held.len()).filter(|&ix| held[ix].active).collect();
    if query.is_empty() {
        return active;
    }
    let mut rows = Vec::new();
    if query.bytes().all(|b| b.is_ascii_digit()) {
        rows = by_number(&active, query, held);
    }
    let lower = query.to_lowercase();
    let mut keyed: Vec<usize> = active
        .iter()
        .copied()
        .filter(|&ix| {
            held[ix]
                .key
                .as_ref()
                .is_some_and(|key| key.to_lowercase().starts_with(&lower))
        })
        .collect();
    keyed.sort_by_key(|&ix| held[ix].key.as_ref().map(|key| key.len()));
    rows.extend(keyed);
    let words: Vec<String> = active
        .iter()
        .map(|&ix| format!("{} {}", held[ix].title, held[ix].about))
        .collect();
    rows.extend(
        filter_indices(query, &words)
            .into_iter()
            .map(|at| active[at]),
    );
    let mut seen = std::collections::HashSet::new();
    rows.retain(|ix| seen.insert(*ix));
    rows
}

/// The positions in `scoped` whose number starts with `number`, the exact
/// one first.
fn by_number(scoped: &[usize], number: &str, held: &[Linkable]) -> Vec<usize> {
    let mut rows: Vec<usize> = scoped
        .iter()
        .copied()
        .filter(|&ix| held[ix].number.to_string().starts_with(number))
        .collect();
    rows.sort_by_key(|&ix| held[ix].number.to_string() != number);
    rows
}

/// What a chip linking an entry paints: its title, and its kind's mark. A
/// run of a session's turns adds the run.
pub(crate) fn preview(url: &str, cx: &App) -> Option<Preview> {
    let reference = url.strip_prefix(crate::view::entry_link::SCHEME)?;
    let resolved = crate::model::workspace::references::resolve_in(reference, cx)?;
    let fallback = match resolved.kind {
        Kind::Session => "Untitled session",
        Kind::Article => "Untitled article",
        Kind::Board => "Untitled board",
    };
    let title = untitled(&resolved.title, fallback);
    let title = match resolved.turns {
        Some(turns) if turns.from == turns.to => format!("{title} · turn {turns}"),
        Some(turns) => format!("{title} · turns {turns}"),
        None => title,
    };
    Some(Preview {
        title: Some(title.into()),
        glyph: Some(crate::view::search::kind_icon(resolved.kind)),
        ..Preview::default()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/mention_rank.rs"]
mod rank_tests;
