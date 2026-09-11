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
}
