//! Project entry kinds and installed apps.

use serde::{Deserialize, Serialize};

/// A project entry, or an app, which belongs
/// to no project and is never numbered. Serialized as its [`Kind::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Session,
    Board,
    Article,
    Canvas,
    Design,
    Table,
    App,
}

impl Kind {
    pub const ALL: [Self; 7] = [
        Self::Session,
        Self::Board,
        Self::Article,
        Self::Canvas,
        Self::Design,
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
            Self::Canvas => "canvas",
            Self::Design => "design",
            Self::Table => "table",
            Self::App => "app",
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}
