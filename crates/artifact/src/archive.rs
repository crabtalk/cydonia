//! Which entries a listing or a search lets through, by whether they are
//! archived.

/// Archived entries left out, let in beside the rest, or alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Archived {
    #[default]
    Exclude,
    Include,
    Only,
}

impl Archived {
    pub const ALL: [Self; 3] = [Self::Exclude, Self::Only, Self::Include];

    /// The word it is written under in a tool call.
    pub fn key(self) -> &'static str {
        match self {
            Self::Exclude => "exclude",
            Self::Include => "include",
            Self::Only => "only",
        }
    }

    /// The filter `word` names, and nothing for one nobody uses.
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_lowercase();
        Self::ALL.into_iter().find(|filter| filter.key() == word)
    }

    /// Whether an entry that is `archived` gets through.
    pub fn keeps(self, archived: bool) -> bool {
        match self {
            Self::Exclude => !archived,
            Self::Include => true,
            Self::Only => archived,
        }
    }
}
