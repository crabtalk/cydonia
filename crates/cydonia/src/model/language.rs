//! Which language a file is written in, and whether this build can colour it.
//!
//! `syntax::registry` holds the one table of names and extensions; grammars
//! reach it through a provider, which [`installed`] registers. This maps the
//! registry's answer onto the three states the app paints from.

use std::{ops::Range, path::Path, sync::OnceLock};

use bezel::theme::HighlightKind;
use syntax::registry::Known;

/// The registry names markdown and carries no grammar for it; `markdown`
/// paints it from its own parser.
const MARKDOWN: &str = "markdown";

/// What a file's name says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// A grammar this build carries, under the name `syntax` knows it by.
    Ready(&'static str),
    /// Markdown, which bezel highlights without a tree-sitter grammar.
    Markdown,
    /// A language this build can name and cannot paint.
    Missing(&'static str),
}

/// Put this build's grammars in the registry. Idempotent, and called from
/// every entry point here rather than from `main`: a lookup that ran first
/// would answer `Missing` for a language this build paints.
fn installed() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(syntax_std::install);
}

/// The language `path` is written in, or `None` where its name names nothing
/// the registry holds.
pub fn of(path: &Path) -> Option<Language> {
    installed();
    let known = syntax::registry::of_path(path)?;
    if known.name() == MARKDOWN {
        return Some(Language::Markdown);
    }
    Some(match known {
        Known::Ready(lang) => Language::Ready(lang.name),
        Known::Named(name) => Language::Missing(name),
    })
}

/// Every language name this build can paint, for the markdown fence highlighter.
pub fn paintable() -> Vec<&'static str> {
    installed();
    syntax::registry::ready()
}

/// The spans `text` is painted with as the contents of `path`, or `None` where
/// nothing here can paint it.
///
/// The single place that answers how a file is coloured; the file view and the
/// diff preview both route through here.
pub fn spans(path: &Path, text: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    match of(path)? {
        Language::Ready(language) => syntax::highlight(text, language),
        Language::Markdown => Some(markdown::source::spans(text)),
        Language::Missing(_) => None,
    }
}
