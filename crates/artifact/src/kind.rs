//! The four things a project holds.

use serde::{Deserialize, Serialize};

/// A session, board, article or table. Serialized as its [`Kind::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Session,
    Board,
    Article,
    Table,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Session, Self::Board, Self::Article, Self::Table];

    /// The word entry numbers are issued under — see
    /// [`crate::project::Project::number`].
    pub fn key(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Board => "board",
            Self::Article => "article",
            Self::Table => "table",
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}
