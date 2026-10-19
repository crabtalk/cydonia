//! The things Cydonia opens: the four a project holds, and apps.

use serde::{Deserialize, Serialize};

/// A session, board, article or table of a project, or an app, which belongs
/// to no project and is never numbered. Serialized as its [`Kind::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Session,
    Board,
    Article,
    Table,
    App,
}

impl Kind {
    pub const ALL: [Self; 5] = [
        Self::Session,
        Self::Board,
        Self::Article,
        Self::Table,
        Self::App,
    ];

    /// The word entry numbers are issued under — see
    /// [`crate::project::Project::number`].
    pub fn key(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Board => "board",
            Self::Article => "article",
            Self::Table => "table",
            Self::App => "app",
        }
    }

    /// The kind whose [`Kind::key`] is `key`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}
