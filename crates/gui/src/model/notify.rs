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

use crate::model::{session::ChatSession, workspace::references};
use bezel::gpui::{App, SharedString, SystemNotification};

/// The prefix a session's notification is tagged with. The tag is the session,
/// so a second turn replaces the first rather than stacking a centre full of
/// them, and the response carries it back — see
/// [`crate::view::window::root::on_notification`].
const TAG: &str = "session:";

/// How much of the agent's last word the body carries.
const BODY: usize = 140;

/// What one session's notifications have said, kept by its pump and shown the
/// session after every change to it.
#[derive(Default)]
pub struct Outbox {
    /// Whether the session was streaming when last seen.
    streaming: bool,
    /// The tool call of the prompt pending when last seen.
    prompt: Option<String>,
    /// Whether the notification under the session's tag is the prompt's.
    asking: bool,
}

impl Outbox {
    /// Say what changed since the session was last seen: a turn finished, or a
    /// prompt opened, replaced, answered or dropped.
    ///
    /// A prompt that goes away takes its notification with it. The tag is the
    /// session's, so a finished turn's notification replaces a prompt's.
    pub fn see(&mut self, chat: &ChatSession, cx: &mut App) {
        let finished = self.streaming && !chat.streaming;
        let prompt = chat.permission.as_ref();
        let changed = prompt.map(|prompt| &prompt.call) != self.prompt.as_ref();
        self.streaming = chat.streaming;
        self.prompt = prompt.map(|prompt| prompt.call.clone());
        if !finished && !changed {
            return;
        }
        if changed && self.asking {
            cx.dismiss_system_notification(&tag(chat));
            self.asking = false;
        }
        let notify = references::current(cx)
            .is_some_and(|workspace| workspace.read(cx).settings.notify_turns);
        if finished && post(chat, body(chat), notify, cx) {
            self.asking = false;
        }
        if changed && let Some(prompt) = prompt {
            let body = clipped(&format!("Needs permission: {}", prompt.title));
            self.asking = post(chat, body, notify, cx);
        }
    }
}

/// Which session a notification came back from, or `None` for one posted by
/// something else.
pub fn session_of(tag: &str) -> Option<u64> {
    tag.strip_prefix(TAG)?.parse().ok()
}

/// Post under `chat`'s tag, if anything should be said at all, and answer
/// whether it was. `notify` is the setting.
fn post(chat: &ChatSession, body: String, notify: bool, cx: &mut App) -> bool {
    if !notify || chat.closed || in_front(cx) {
        return false;
    }
    cx.show_system_notification(SystemNotification {
        tag: SharedString::from(tag(chat)),
        title: SharedString::from(chat.label()),
        body: SharedString::from(body),
        actions: Vec::new(),
    });
    true
}

fn tag(chat: &ChatSession) -> String {
    format!("{TAG}{}", chat.id)
}

/// Whether one of the app's windows is the one the user is in.
///
/// Not [`App::active_window`]: on macOS it reads `mainWindow`, which stays set
/// after the app moves to the background.
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
