//! Language detection and syntax highlighting. With the `desktop` feature,
//! grammars are WASM downloaded on demand; without it only Markdown is
//! recognised, and a fence is painted only when its spans were worked out
//! ahead of time — see [`Ahead::prepare`].

use bezel::{gpui::App, theme::HighlightKind};
use std::{ops::Range, path::Path};

#[cfg(not(feature = "desktop"))]
mod ahead;
#[cfg(feature = "desktop")]
mod installed;
#[cfg(feature = "desktop")]
mod provider;

#[cfg(not(feature = "desktop"))]
pub use ahead::{Ahead, NoGrammar, Prepared};
#[cfg(feature = "desktop")]
pub use installed::Installed;

/// The registry names markdown and carries no grammar for it; `markdown`
/// paints it from its own parser.
const MARKDOWN: &str = "markdown";

/// Where this build's grammars come from.
#[cfg(feature = "desktop")]
pub type Here = Installed;
#[cfg(not(feature = "desktop"))]
pub type Here = Ahead;

/// A grammar the catalogue can install, in this build.
#[cfg(feature = "desktop")]
pub type Spec = provider::Spec;
#[cfg(not(feature = "desktop"))]
pub type Spec = NoGrammar;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Missing,
    Downloading { received: u64, total: u64 },
    Checking,
    Ready,
    Failed(String),
}

impl Status {
    pub fn active(&self) -> bool {
        matches!(self, Self::Downloading { .. } | Self::Checking)
    }
}

/// What a file's name says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// An installed grammar, under the name `syntax` knows it by.
    Ready(&'static str),
    /// Markdown, which bezel highlights without a tree-sitter grammar.
    Markdown,
    /// A known language whose grammar is not installed.
    Missing(&'static str),
}

/// What can tell a file's language and paint it.
pub trait Grammars {
    /// The language `path` is written in, or `None` where its name names
    /// nothing known.
    fn of(path: &Path) -> Option<Language>;

    /// The languages [`Grammars::highlight`] can answer for right now.
    fn paintable() -> Vec<&'static str>;

    /// Every language the Markdown fence picker lists.
    fn offerable() -> Vec<&'static str>;

    /// Make the grammars `labels` name paintable, where this build can.
    fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App);

    /// The spans `source` is painted with as `language`. Shared by file views,
    /// diffs, and Markdown fences.
    fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>>;
}

/// A grammar the catalogue can install.
pub trait Installable: Sized + 'static {
    /// The grammar under this catalogue name.
    fn named(name: &str) -> Option<&'static Self>;

    /// The grammar a fence's label means, following aliases.
    fn resolve(label: &str) -> Option<&'static Self>;

    fn status(&self) -> Status;

    /// Called only by the user's Install/Retry action. Concurrent views share
    /// one download.
    fn start_install(&'static self);
}

/// The spans `text` is painted with as the contents of `path`, or `None` where
/// nothing here can paint it.
///
/// The single place that answers how a file is coloured; the file view and the
/// diff preview both route through here.
pub fn spans(path: &Path, text: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    match Here::of(path)? {
        Language::Ready(language) => Here::highlight(language, text),
        Language::Markdown => Some(markdown::source::spans(text)),
        Language::Missing(_) => None,
    }
}
