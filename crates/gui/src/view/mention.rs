//! What `@` in an article lists: the active project's sessions, boards and
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
struct Linkable {
    title: String,
    about: String,
    url: String,
    touched: u128,
}

/// The entries `@` can link, most recently touched first. Rebuilt whenever
/// the workspace changes.
#[derive(Default)]
pub(crate) struct Linkables(Vec<Linkable>);

impl Global for Linkables {}

/// The active project's entries, as what `@` lists. Installed with
/// `cx.set_global`.
pub(crate) fn read(workspace: &Workspace) -> Linkables {
    let mut held = Vec::new();
    if let Some(project) = workspace.active_project()
        && let Some(name) = project.path.file_name().map(|name| name.to_string_lossy())
    {
        let reference = |number: u64| link(&format!("{name}#{number}"));
        held.extend(project.sessions.iter().filter_map(|chat| {
            let number = chat.number?;
            Some(Linkable {
                title: untitled(&chat.title, "Untitled session"),
                about: format!("Session · {} · #{number}", chat.entry.name),
                url: reference(number),
                touched: chat.touched(),
            })
        }));
        held.extend(project.articles.iter().filter_map(|article| {
            let number = article.number?;
            Some(Linkable {
                title: untitled(&article.title, "Untitled article"),
                about: format!("Article · #{number}"),
                url: reference(number),
                touched: article.touched,
            })
        }));
        held.extend(project.boards.iter().filter_map(|board| {
            let number = board.number?;
            Some(Linkable {
                title: untitled(&board.name, "Untitled board"),
                about: format!("Board · #{number}"),
                url: reference(number),
                touched: board.touched,
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

/// The mention source cydonia installs: the query ranked against each entry's
/// title, kind and number.
pub(crate) fn source(query: &str, cx: &App) -> Vec<Mention> {
    let Some(Linkables(held)) = cx.try_global::<Linkables>() else {
        return Vec::new();
    };
    let order: Vec<usize> = match query.trim() {
        "" => (0..held.len()).collect(),
        query => {
            let words: Vec<String> = held
                .iter()
                .map(|linkable| format!("{} {}", linkable.title, linkable.about))
                .collect();
            filter_indices(query, &words)
        }
    };
    order
        .into_iter()
        .take(SHOWN)
        .map(|ix| {
            let linkable = &held[ix];
            Mention {
                label: SharedString::from(linkable.title.clone()),
                description: Some(SharedString::from(linkable.about.clone())),
                url: linkable.url.clone(),
            }
        })
        .collect()
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
