//! What `@` in an article lists: the open projects' sessions, boards and
//! articles, linked as `cydonia://<project>#<number>` chips — and the title a
//! chip of one paints: the kind's mark and the title.

use crate::{
    model::workspace::Workspace,
    view::{entry_link::link, search::kind_icon},
};
use artifact::{
    reference::{self, Prefix},
    search::Kind,
};
use bezel::ui::icons::Icon;
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
            |icon: Icon, title: String, about: String, number: u64, touched: u128| Linkable {
                icon,
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
                workspace
                    .agent_icon(&chat.entry.name)
                    .unwrap_or_else(|| kind_icon(Kind::Session)),
                untitled(&chat.title, "Untitled session"),
                format!("Session · {} · {}", chat.entry.name, shown(number)),
                number,
                chat.touched(),
            ))
        }));
        held.extend(project.articles.iter().filter_map(|article| {
            let number = article.number?;
            Some(linkable(
                kind_icon(Kind::Article),
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
                    kind_icon(Kind::Board),
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
                icon: Some(linkable.icon.clone()),
                label: SharedString::from(linkable.title.clone()),
                description: Some(SharedString::from(linkable.about.clone())),
                url: linkable.url.clone(),
            }
        })
        .collect()
}

/// What `query` lists, as positions in `held`, best first.
///
/// Read as the start of a reference first — see [`reference::partial`]: a
/// number prefix lists the entries whose number starts with it, a key prefix
/// the boards whose key does, each in the project named before the `#` or
/// the active one. A bare query also matches the active project's titles
/// after those. An exact number or key leads its group; ties keep the most
/// recently touched first.
pub(crate) fn rank(query: &str, held: &[Linkable]) -> Vec<usize> {
    let query = query.trim();
    let partial = reference::partial(query);
    let project = partial.as_ref().and_then(|partial| partial.project);
    let scoped: Vec<usize> = (0..held.len())
        .filter(|&ix| match project {
            None => held[ix].active,
            Some(name) => held[ix].project.eq_ignore_ascii_case(name),
        })
        .collect();
    if query.is_empty() {
        return scoped;
    }
    let mut rows = match partial.map(|partial| partial.target) {
        Some(Prefix::Number(number)) => by_prefix(&scoped, number, held, |linkable| {
            Some(linkable.number.to_string())
        }),
        Some(Prefix::Key(key)) => by_prefix(&scoped, key, held, |linkable| {
            linkable.key.as_ref().map(|key| key.to_lowercase())
        }),
        None => Vec::new(),
    };
    if !query.contains('#') {
        let words: Vec<String> = scoped
            .iter()
            .map(|&ix| format!("{} {}", held[ix].title, held[ix].about))
            .collect();
        rows.extend(
            filter_indices(query, &words)
                .into_iter()
                .map(|at| scoped[at]),
        );
    }
    let mut seen = std::collections::HashSet::new();
    rows.retain(|ix| seen.insert(*ix));
    rows
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
        glyph: Some(kind_icon(resolved.kind)),
        ..Preview::default()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/mention_rank.rs"]
mod rank_tests;
