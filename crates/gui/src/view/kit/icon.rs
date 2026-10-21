//! The marks entries wear.

use crate::model::{
    session::ChatSession,
    workspace::{Showing, Workspace},
};
use artifact::{kind::Kind, space::Member};
use bezel::{
    gpui::App,
    ui::icons::{self, Icon},
};

/// The mark an entry of `kind` wears wherever it is listed or linked.
pub(crate) fn kind_icon(kind: Kind) -> Icon {
    match kind {
        Kind::Session => icons::social::MessageCircle.into(),
        Kind::Board => icons::development::SquareKanban.into(),
        Kind::Article => icons::files::FileText.into(),
        Kind::Table => icons::files::Table2.into(),
        Kind::App => icons::development::AppWindow.into(),
    }
}

/// The mark a session wears: its agent's, or a session's where the agent has
/// none.
pub(crate) fn session_icon(chat: &ChatSession, workspace: &Workspace) -> Icon {
    workspace
        .agent_icon(&chat.entry.name)
        .unwrap_or_else(|| kind_icon(Kind::Session))
}

/// The mark the entry `member` names wears: a session's agent, else its kind's.
pub(crate) fn entry_icon(member: &Member, workspace: &Workspace, cx: &App) -> Icon {
    match workspace.showing_of(member, cx) {
        Some((_, Showing::Session(id))) => match workspace.session(id, cx) {
            Some(chat) => session_icon(chat, workspace),
            None => kind_icon(Kind::Session),
        },
        _ => kind_icon(member.kind()),
    }
}

/// The mark the entry `member` names wears on a card of its own, the deck's:
/// one without a frame where its kind's mark is drawn in one, since the card
/// is the frame.
pub(crate) fn card_icon(member: &Member, workspace: &Workspace, cx: &App) -> Icon {
    match member.kind() {
        Kind::Board => icons::glyph::Kanban.into(),
        Kind::Article => icons::glyph::TextAlignStart.into(),
        Kind::Session | Kind::Table | Kind::App => entry_icon(member, workspace, cx),
    }
}
