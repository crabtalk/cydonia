//! What the app says to the system while nobody is looking at it: a turn has
//! finished, or an agent has stopped to ask for permission.
//!
//! Only ever posted with the app in the background. A turn you watched finish
//! is one you already know about, and a notification for it is noise in a
//! centre you have to clear by hand.
//!
//! Delivery is the platform's and is allowed to fail: macOS asks for
//! authorisation on the first post and drops everything if it is refused, and a
//! build running outside an app bundle — `cargo run` — has no identity to post
//! under, so nothing arrives. Nothing here treats any of that as an error.

use crate::model::session::ChatSession;
use bezel::gpui::{App, SharedString, SystemNotification};

/// The prefix a session's notification is tagged with. The tag is the session,
/// so a second turn replaces the first rather than stacking a centre full of
/// them, and the response carries it back — see
/// [`crate::view::window::root::on_notification`].
const TAG: &str = "session:";

/// How much of the agent's last word the body carries.
const BODY: usize = 140;

/// Tell the system a turn has finished, if anything should be said at all.
pub fn turn_finished(chat: &ChatSession, notify: bool, cx: &mut App) {
    post(chat, body(chat), notify, cx);
}

/// Tell the system an agent is waiting on a permission prompt, under the same
/// rule as [`turn_finished`].
///
/// This is the one a background session most needs said: a finished turn can
/// wait to be read, but a prompt holds the turn until someone answers it. The
/// tag is the session's, so the turn's own notification replaces this one once
/// the prompt is answered and the turn ends.
pub fn permission_requested(chat: &ChatSession, notify: bool, cx: &mut App) {
    let Some(prompt) = &chat.permission else {
        return;
    };
    let body = clipped(&format!("Needs permission: {}", prompt.title));
    post(chat, body, notify, cx);
}

/// Which session a notification came back from, or `None` for one posted by
/// something else.
pub fn session_of(tag: &str) -> Option<u64> {
    tag.strip_prefix(TAG)?.parse().ok()
}

/// `notify` is the setting; the app being in the background is the rest of the
/// answer.
fn post(chat: &ChatSession, body: String, notify: bool, cx: &mut App) {
    if !notify || chat.closed || in_front(cx) {
        return;
    }
    cx.show_system_notification(SystemNotification {
        tag: SharedString::from(format!("{TAG}{}", chat.id)),
        title: SharedString::from(chat.label()),
        body: SharedString::from(body),
        actions: Vec::new(),
    });
}

/// Whether one of the app's windows is the one the user is in.
///
/// Not [`App::active_window`]: on macOS it reads `mainWindow`, which stays set
/// after the app moves to the background, so it never reported the app as away
/// and nothing was ever posted.
fn in_front(cx: &mut App) -> bool {
    cx.windows().into_iter().any(|handle| {
        handle
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap_or(false)
    })
}

/// What the turn left behind, as one line: the failure if it ended in one, else
/// the agent's last word, else that it is done — an agent can finish a turn
/// having said nothing but tool calls.
fn body(chat: &ChatSession) -> String {
    use artifact::session::chat::ChatItem;
    for item in chat.items.iter().rev() {
        match item {
            ChatItem::Notice { text, failed: true } => return clipped(text),
            ChatItem::Agent(text) if !text.trim().is_empty() => return clipped(text),
            _ => {}
        }
    }
    "Finished".to_owned()
}

/// The first line of `text`, short enough for a notification to show whole.
fn clipped(text: &str) -> String {
    artifact::text::clip(text.trim().lines().next().unwrap_or_default(), BODY)
}
