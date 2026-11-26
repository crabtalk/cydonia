use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: u64,
    pub note: String,
    /// The session turn that produced it, as a `cydonia://` link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    #[serde(default)]
    pub resolved: bool,
    /// What was said after the note: follow-ups asked on it, and their
    /// answers, oldest first.
    #[serde(default, rename = "reply", skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<Reply>,
}

/// One message after a comment's note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub from: Author,
    pub text: String,
    /// The session turn that produced it, as a `cydonia://` link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
}

/// Who wrote a [`Reply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Author {
    /// The person asking a follow-up.
    Reader,
    /// The agent, answering one.
    Agent,
}

impl Thread {
    pub fn new(id: u64, note: String, turn: Option<String>) -> Self {
        Self {
            id,
            note,
            turn,
            resolved: false,
            replies: Vec::new(),
        }
    }

    pub fn fresh_id(now: u64, ids: impl Iterator<Item = u64>) -> u64 {
        ids.max().map_or(now, |top| now.max(top.saturating_add(1)))
    }

    pub fn reply(&mut self, from: Author, text: String, turn: Option<String>) {
        self.replies.push(Reply { from, text, turn });
    }

    /// How many replies an agent has written on the note.
    pub fn agent_replies(&self) -> usize {
        self.replies
            .iter()
            .filter(|reply| reply.from == Author::Agent)
            .count()
    }

    /// The note's first line with any text on it.
    pub fn headline(&self) -> &str {
        self.note
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
    }
}
