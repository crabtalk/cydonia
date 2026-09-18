//! What the app says to the system while nobody is looking at it: a turn has
//! finished.
//!
//! Only ever posted with the app in the background. A turn you watched finish
//! is one you already know about, and a notification for it is noise in a
//! centre you have to clear by hand.
//!
//! Delivery is the platform's and is allowed to fail: macOS asks for
//! authorisation on the first post and drops everything if it is refused, and a
//! build running outside an app bundle — `cargo run` — has no identity to post
//! under, so nothing arrives. Nothing here treats any of that as an error.

use crate::{model::session::ChatSession, view::root::Cydonia};
use bezel::gpui::{App, SharedString, SystemNotification};

/// The prefix a session's notification is tagged with. The tag is the session,
/// so a second turn replaces the first rather than stacking a centre full of
/// them, and the response carries it back — see [`on_response`].
const TAG: &str = "session:";

/// How much of the agent's last word the body carries.
const BODY: usize = 140;

/// Tell the system a turn has finished, if anything should be said at all.
///
/// `notify` is the setting; the window being in the background is the rest of
/// the answer, and [`App::active_window`] is empty exactly when the app is not
/// the one in front.
pub fn turn_finished(chat: &ChatSession, notify: bool, cx: &mut App) {
    if !notify || chat.closed || cx.active_window().is_some() {
        return;
    }
    cx.show_system_notification(SystemNotification {
        tag: SharedString::from(format!("{TAG}{}", chat.id)),
        title: SharedString::from(chat.label()),
        body: SharedString::from(body(chat)),
        actions: Vec::new(),
    });
}

/// Which session a notification came back from, or `None` for one posted by
/// something else.
pub fn session_of(tag: &str) -> Option<u64> {
    tag.strip_prefix(TAG)?.parse().ok()
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
    let line = text.trim().lines().next().unwrap_or_default().trim();
    match line.char_indices().nth(BODY) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// What a press on one of these does: bring the app back, and open the session
/// that finished.
///
/// Registered once, at startup. The window it lands on is whichever one is
/// showing the project that session is in; a notification whose session has
/// since gone opens nothing but the app.
pub fn on_response(cx: &mut App) {
    cx.on_system_notification_response(|response, cx| {
        let Some(id) = session_of(&response.tag) else {
            return;
        };
        cx.activate(true);
        for window in cx.windows() {
            let Some(window) = window.downcast::<Cydonia>() else {
                continue;
            };
            let _ = window.update(cx, |root, window, cx| root.select_session(id, window, cx));
        }
    });
}
