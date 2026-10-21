//! The orb a session's turn in flight wears, wherever the session is drawn:
//! its transcript, its sidebar row, the composer's activity line, a busy
//! card.

use crate::model::session::ChatSession;
use artifact::session::chat::ChatItem;
use bezel::{
    agent::orbs::{OrbSize, OrbState, engine::Frame, orb_element},
    gpui::{AnyElement, Context, prelude::*},
    motion::Painter,
};
use std::{
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    rc::Rc,
    time::Duration,
};

/// What the orb is drawn on: 30 frames a second while a turn is in flight,
/// claimed for a third of a second at a time and renewed by the render it
/// drives. A turn that ends stops rendering the row, the claim lapses, and the
/// app's clock parks itself.
const ORB_FPS: f32 = 30.;
const ORB_LEASE: Duration = Duration::from_millis(300);

/// The still frame an orb holds where motion is turned off — the engine's own
/// convention, and a pose rather than the empty t = 0 arrangement.
const ORB_STILL: f32 = 0.6;

/// A turn in flight, as its orb is drawn: which of the twelve, how long it has
/// been running, and the buffer it paints into.
pub(crate) struct Working {
    pub(crate) state: OrbState,
    pub(crate) since: Duration,
    pub(crate) frame: Rc<RefCell<Frame>>,
}

impl Working {
    /// The turn `chat` has in flight.
    pub(crate) fn of(chat: &ChatSession) -> Self {
        Self {
            state: of(chat),
            since: chat.elapsed().unwrap_or_default(),
            frame: chat.mark.clone(),
        }
    }

    pub(crate) fn draw<V: 'static>(&self, cx: &mut Context<V>) -> AnyElement {
        draw(self.state, self.since, &self.frame, cx)
    }
}

/// Which orb a session's turn in flight wears, off the question's hash as
/// [`crate::view::component::transcript::working_word`] is: a shape rerolled per frame would be twelve loaders playing at once,
/// and one picked per place would have the sidebar and the transcript showing
/// the same turn as two different things.
///
/// The word and the shape are picked apart rather than paired — there are
/// forty of one and twelve of the other, and pairing them would cost the words
/// their variety.
pub(crate) fn of(chat: &ChatSession) -> OrbState {
    // The turn in flight opened at the last thing asked — which is where
    // `turns` starts one too, and 0 for a session that was never asked
    // anything.
    let at = chat
        .items
        .iter()
        .rposition(|item| matches!(item, ChatItem::User(_)))
        .unwrap_or(0);
    let question = chat
        .items
        .get(at)
        .and_then(ChatItem::text)
        .unwrap_or_default();
    of_text(question)
}

/// The same choice, made from any text — a card's own words, where there is no
/// session and so no question to make it from.
///
/// Stable for the same text: an orb that changed shape between frames would
/// read as the work changing.
pub(crate) fn of_text(asked: &str) -> OrbState {
    OrbState::ALL_STATES[asked_hash(asked) % OrbState::ALL_STATES.len()]
}

/// That orb, drawn, at the time the turn has been running — unbounded, which
/// is what the engine wants, and starting from nothing every turn.
///
/// The orb paints one frame of a clock it does not keep, so this asks for the
/// frames itself. The claim is renewed by the render it drives rather than
/// held anywhere, so it lapses on its own the moment the orb stops being
/// drawn.
pub(crate) fn draw<V: 'static>(
    state: OrbState,
    since: Duration,
    frame: &Rc<RefCell<Frame>>,
    cx: &mut Context<V>,
) -> AnyElement {
    Painter::of(cx).lease(ORB_FPS, ORB_LEASE, cx);
    let t = match cx.reduce_motion() {
        true => ORB_STILL,
        false => since.as_secs_f32(),
    };
    orb_element(state, OrbSize::Inline, t, frame).into_any_element()
}

/// The question, as a number to pick with.
pub(crate) fn asked_hash(question: &str) -> usize {
    let mut hash = DefaultHasher::new();
    question.hash(&mut hash);
    hash.finish() as usize
}
