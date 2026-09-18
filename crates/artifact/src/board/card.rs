//! A card: what needs doing, the session it was handed to, and how that work
//! is going.

use serde::{Deserialize, Serialize};

/// How the work on a card is going, as whoever is doing it says.
///
/// Not where the card sits: that is the column, which the reader names and
/// rearranges, and a card in `Doing` that an agent has not picked up yet is a
/// real state the two together can say and neither can alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Being worked on now.
    Busy,
    /// Cannot go on.
    Blocked,
    /// The work is finished.
    Done,
}

impl Status {
    pub const ALL: [Self; 3] = [Self::Busy, Self::Blocked, Self::Done];

    /// The word it is written under, in a file and in a tool call.
    pub fn key(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    /// The status that word names, and nothing for one nobody uses. `none`
    /// is not handled here: taking a status off is the absence of one, and a
    /// caller saying so is saying `None`.
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_lowercase();
        Self::ALL.into_iter().find(|status| status.key() == word)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    /// What names this card, for as long as it exists. Defaulted, and minted
    /// by the read that finds it missing — see [`super::Board::mint_ids`].
    #[serde(default)]
    pub id: String,
    /// The `12` in `ROAD-12`: short enough to say to an agent, and so not
    /// unique for all time the way [`Card::id`] is. Minted on read when
    /// missing — see [`super::Board::mint_ids`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<u64>,
    pub text: String,
    /// The session this card was dispatched to, by the id it is filed under.
    /// Persisted, so ▶/💬 still tells the truth after a quit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// How the work is going — see [`Status`]. Nothing for a card nobody has
    /// said anything about, which is most of them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

impl Card {
    /// Under an id and a handle its board has found free — see
    /// [`super::Board::add_card`].
    pub fn new(id: String, handle: u64, text: String) -> Self {
        Self {
            id,
            handle: Some(handle),
            text,
            session: None,
            status: None,
        }
    }
}
