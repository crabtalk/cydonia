//! What a transcript is made of: one variant per thing a turn can put on
//! screen, and the two small states a tool call and a plan step move through.
//!
//! This is the shape [`super::record::Record`] writes and reads back, so it is
//! the shape anything reading a session off disk has to understand. Kept apart
//! from the live session for that reason — a session is a connection and a
//! process, and neither of those is in the file.

use cacp::schema::ToolKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Success,
    Failure,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PlanStatus {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum ChatItem {
    User(String),
    Agent(String),
    Thinking {
        text: String,
        done: bool,
    },
    Tool {
        id: String,
        kind: ToolKind,
        label: String,
        status: ToolStatus,
        output: String,
    },
    /// Something the session has to say for itself: a stop reason, or a
    /// failure. `failed` picks which strip it paints as.
    Notice {
        text: String,
        failed: bool,
    },
    /// What the agent process wrote to its stderr rather than said in
    /// protocol — a warning from the runtime under it, or the reason it never
    /// got as far as speaking at all. An execution and its output, so it is
    /// kept as one: `command` is what was run, `output` what came back.
    Process {
        command: String,
        output: String,
    },
}

/// Whether nothing has been said in a transcript yet.
///
/// Not the same as holding no items. An agent writes to stderr as it starts —
/// a deprecation warning, a runtime's banner — and every line of that is an
/// item before anybody has typed a word. It is the process talking about
/// itself rather than a conversation, so a session carrying only that is still
/// one nothing has been said in: it keeps its empty state, and it mints no
/// file.
///
/// A [`ChatItem::Notice`] does count. A connection that failed is the app
/// saying so, and that is worth the transcript and the file both.
pub fn nothing_said(items: &[ChatItem]) -> bool {
    items
        .iter()
        .all(|item| matches!(item, ChatItem::Process { .. }))
}

/// A transcript split into turns, each the item range from one question to
/// the next. The leading chunk before the first question is a turn of its
/// own. Runs of nothing but process output are not turns.
///
/// Turn `n` here is turn `n` on the transcript's rail and in the tools.
pub fn turns(items: &[ChatItem]) -> Vec<std::ops::Range<usize>> {
    let mut turns = Vec::new();
    let mut start = 0;
    for ix in 1..=items.len() {
        if ix < items.len() && !matches!(items[ix], ChatItem::User(_)) {
            continue;
        }
        if !nothing_said(&items[start..ix]) {
            turns.push(start..ix);
        }
        start = ix;
    }
    turns
}
