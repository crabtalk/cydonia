//! The mark a session's status stands in an icon slot as, wherever the window
//! shows the session without opening it: its sidebar row, its tab, the space
//! holding it.

use crate::{
    model::session::{ChatSession, Status},
    view::kit::orb,
};
use bezel::{
    gpui::{AnyElement, Context, prelude::*, px},
    theme::Theme,
    ui::icons::{self, glyph},
};

/// The diameter both still marks are drawn at: Lucide's circle fills the same
/// box in each.
const STILL: f32 = 12.;

/// A session's status, read off the session to be drawn later in a view's
/// context. An orb for one under way, a still mark for one waiting on you.
pub(crate) enum Mark {
    Orb(orb::Working),
    Failed,
    Review,
}

impl Mark {
    /// The session's mark, or `None` where it is idle and keeps its own.
    pub(crate) fn of(chat: &ChatSession) -> Option<Self> {
        match chat.status() {
            Status::Asking => Some(Self::Orb(orb::Working {
                state: orb::ASKING,
                ..orb::Working::of(chat)
            })),
            Status::Working => Some(Self::Orb(orb::Working::of(chat))),
            Status::Failed => Some(Self::Failed),
            Status::Review => Some(Self::Review),
            Status::Idle => None,
        }
    }

    /// The mark of the most pressing of `chats`, or `None` where all are idle.
    pub(crate) fn of_most_pressing<'a>(
        chats: impl IntoIterator<Item = &'a ChatSession>,
    ) -> Option<Self> {
        chats
            .into_iter()
            .min_by_key(|chat| chat.status())
            .and_then(Self::of)
    }

    pub(crate) fn draw<V: 'static>(&self, cx: &mut Context<V>) -> AnyElement {
        let theme = Theme::of(cx);
        match self {
            Self::Orb(working) => working.draw(cx),
            Self::Failed => icons::icon(glyph::CircleAlert)
                .size(px(STILL))
                .text_color(theme.danger)
                .into_any_element(),
            Self::Review => icons::solid(glyph::Circle)
                .size(px(STILL))
                .text_color(theme.accent)
                .into_any_element(),
        }
    }
}
