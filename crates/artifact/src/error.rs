//! The failures more than one front end reports, each worded once.
//!
//! A front end maps a variant onto how it reports a failure; it does not write
//! the words again. Context a front end adds goes on a line of its own after
//! these.

use crate::kind::Kind;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// `named` is no `what` in the project.
    #[error("no {what} {named} in this project")]
    NotFound { what: &'static str, named: String },

    /// `named` was found and has since gone.
    #[error("{named} no longer exists")]
    Gone { named: String },

    /// `what` could not be `act` — read, written, renamed and so on.
    #[error("{what} cannot be {act} — {cause}")]
    Failed {
        what: String,
        act: Act,
        cause: anyhow::Error,
    },

    /// `path` is not among the projects cydonia has open, which are `open`.
    #[error("{}", closed(path, open))]
    ProjectClosed { path: PathBuf, open: Vec<PathBuf> },

    /// No project cydonia has open goes by `name`.
    #[error("{}", unnamed(name, open))]
    NoProjectNamed { name: String, open: Vec<PathBuf> },

    #[error("a key needs at least one letter or digit")]
    KeyEmpty,

    #[error("{key} is another board's key here")]
    KeyTaken { key: String },

    /// `named` names a heading in an entry that is not an article.
    #[error("{named} names a heading, which only an article has")]
    HeadingNotHeld { named: String },

    /// `named` names part of an entry of a kind that has no parts.
    #[error(
        "{named} names part of a {kind}, and only an article's lines or a session's turns can be named"
    )]
    PartNotHeld { named: String, kind: Kind },

    /// `named` names lines starting past the article's last, `lines`.
    #[error("{named} starts past the article's last line, {lines}")]
    PastLastLine { named: String, lines: usize },

    /// `named` names a heading the article does not have; it has `anchors`.
    #[error("{}", headless(named, anchors))]
    NoHeading { named: String, anchors: Vec<String> },

    /// A write refused because what it was based on is no longer what the
    /// backend holds: another writer got there first. Read again, apply the
    /// change again, and save that.
    #[error("it changed since it was read")]
    Stale,

    /// A failure underneath, in its own words.
    #[error(transparent)]
    Other(anyhow::Error),
}

/// An [`Error`] that went through `anyhow` comes back as itself.
impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        error.downcast::<Error>().unwrap_or_else(Self::Other)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Other(error.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Other(error.into())
    }
}

/// What was being done when [`Error::Failed`] happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Read,
    Written,
    Renamed,
    Deleted,
    Made,
    Moved,
    Named,
}

impl std::fmt::Display for Act {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Written => "written",
            Self::Renamed => "renamed",
            Self::Deleted => "deleted",
            Self::Made => "made",
            Self::Moved => "moved",
            Self::Named => "named",
        })
    }
}

impl Error {
    /// [`Error::Failed`], from any error.
    pub fn failed(what: impl Into<String>, act: Act, cause: impl Into<anyhow::Error>) -> Self {
        Self::Failed {
            what: what.into(),
            act,
            cause: cause.into(),
        }
    }
}

fn listed(open: &[PathBuf]) -> String {
    open.iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn closed(path: &std::path::Path, open: &[PathBuf]) -> String {
    match open.is_empty() {
        true => format!(
            "cydonia has no project open, so {} is not one to work in",
            path.display()
        ),
        false => format!(
            "cydonia does not have {} open — it has {}",
            path.display(),
            listed(open)
        ),
    }
}

fn unnamed(name: &str, open: &[PathBuf]) -> String {
    match open.is_empty() {
        true => format!("no open project is named {name}, and cydonia has none open"),
        false => format!(
            "no open project is named {name} — cydonia has {}",
            listed(open)
        ),
    }
}

fn headless(named: &str, anchors: &[String]) -> String {
    match anchors.is_empty() {
        true => format!("{named} names a heading, and the article has none"),
        false => format!(
            "{named} names no heading — the article's are {}",
            anchors.join(", ")
        ),
    }
}
