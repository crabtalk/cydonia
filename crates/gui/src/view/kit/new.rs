//! The "New …" choices a project offers: a session on each agent, a board, an
//! article, a table — as far as the settings switch them on.

use crate::{
    model::workspace::Workspace,
    view::kit::menu::{self, Act},
};
use bezel::{
    gpui::{Context, Window},
    ui::{
        icons::{self, Icon},
        menu::Item,
    },
};

/// What a "New …" choice makes.
#[derive(Clone, Copy)]
pub(crate) enum New {
    /// On the agent at this index of the settings, or on the preferred one.
    Session(Option<usize>),
    Board,
    Article,
    Table,
}

impl New {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Session(_) => "New session",
            Self::Board => "New board",
            Self::Article => "New article",
            Self::Table => "New table",
        }
    }

    pub(crate) fn icon(self) -> Icon {
        match self {
            Self::Session(_) => icons::social::MessageCirclePlus.into(),
            Self::Board => icons::development::SquareKanban.into(),
            Self::Article => icons::files::FilePlus.into(),
            Self::Table => icons::files::Table2.into(),
        }
    }
}

/// What `workspace` offers to make, in menu order.
pub(crate) struct Offered {
    pub(crate) sessions: bool,
    /// The agents a session can open on, by name and mark.
    pub(crate) agents: Vec<(String, Icon)>,
    /// Boards, articles and tables, where switched on.
    pub(crate) kinds: Vec<New>,
}

pub(crate) fn offered(workspace: &Workspace) -> Offered {
    let features = &workspace.settings.features;
    let agents = workspace
        .settings
        .agents
        .iter()
        .map(|entry| {
            let icon = workspace
                .agent_icon(&entry.name)
                .unwrap_or_else(|| icons::social::MessageCircle.into());
            (entry.name.clone(), icon)
        })
        .collect();
    let kinds = [
        (features.boards, New::Board),
        (true, New::Article),
        (features.tables, New::Table),
    ]
    .into_iter()
    .filter_map(|(on, kind)| on.then_some(kind))
    .collect();
    Offered {
        sessions: features.sessions,
        agents,
        kinds,
    }
}

/// The menu rows for `offered`: "New session", or a submenu of agents where
/// there is more than one, then a row a kind. `act` is what picking one does.
/// A lone "New session" is [`New::Session`] of `None`.
pub(crate) fn rows<V: 'static>(
    offered: Offered,
    act: impl Fn(New) -> Box<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
) -> Vec<(Item, Act<V>)> {
    let mut rows = Vec::new();
    if offered.sessions && offered.agents.len() > 1 {
        let picks = offered
            .agents
            .into_iter()
            .enumerate()
            .map(|(at, (name, icon))| {
                menu::row(
                    Item::action(name).with_icon(icon),
                    act(New::Session(Some(at))),
                )
            })
            .collect();
        let session = New::Session(None);
        rows.push(menu::submenu(session.label(), session.icon(), picks));
    } else if offered.sessions {
        rows.push(entry(New::Session(None), &act));
    }
    rows.extend(offered.kinds.into_iter().map(|kind| entry(kind, &act)));
    rows
}

fn entry<V: 'static>(
    kind: New,
    act: &impl Fn(New) -> Box<dyn Fn(&mut V, &mut Window, &mut Context<V>)>,
) -> (Item, Act<V>) {
    menu::row(Item::action(kind.label()).with_icon(kind.icon()), act(kind))
}
