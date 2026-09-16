//! Language detection and on-demand WASM syntax highlighting.

use std::{ops::Range, path::Path, sync::OnceLock};

use bezel::theme::HighlightKind;
use syntax::registry::Known;

mod provider;
pub use provider::{Status, available, start_install, status};

/// The registry names markdown and carries no grammar for it; `markdown`
/// paints it from its own parser.
const MARKDOWN: &str = "markdown";

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

/// Register verified cached grammars once, without downloading anything.
fn installed() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(provider::initialize);
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

/// Installed languages for the Markdown fence picker.
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
        Language::Ready(language) => highlight(language, text),
        Language::Markdown => Some(markdown::source::spans(text)),
        Language::Missing(_) => None,
    }
}

/// Shared by file views, diffs, and Markdown fences.
pub fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    installed();
    provider::ensure_runtime().ok()?;
    syntax::highlight(source, language)
}
