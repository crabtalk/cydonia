//! Grammars for a build without the `desktop` feature: Markdown, and fences
//! whose spans were worked out ahead of time.

use super::{Grammars, HighlightKind, Installable, Language, MARKDOWN, Status};
use bezel::gpui::App;
use std::{ops::Range, path::Path, sync::OnceLock};

/// A fence painted ahead of time: its language, its source, and the spans as
/// byte offsets into that source.
pub type Prepared = (
    &'static str,
    &'static str,
    &'static [(usize, usize, HighlightKind)],
);

static PREPARED: OnceLock<&'static [Prepared]> = OnceLock::new();

/// Markdown and the fences handed to [`Ahead::prepare`].
pub struct Ahead;

/// No grammar is in the catalogue without the `desktop` feature.
pub enum NoGrammar {}

impl Ahead {
    /// The fences this build can paint. Set once; a second call is ignored.
    pub fn prepare(table: &'static [Prepared]) {
        let _ = PREPARED.set(table);
    }
}

impl Grammars for Ahead {
    fn of(path: &Path) -> Option<Language> {
        let ext = path.extension()?.to_str()?;
        matches!(ext, "md" | "markdown").then_some(Language::Markdown)
    }

    fn paintable() -> Vec<&'static str> {
        Vec::new()
    }

    fn offerable() -> Vec<&'static str> {
        vec![MARKDOWN]
    }

    fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App) {
        let _ = (labels.into_iter(), cx);
    }

    /// Matched on the source with trailing whitespace ignored, which is the
    /// one way a fence's body and the file it was read from differ.
    fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
        if language == MARKDOWN {
            return Some(markdown::source::spans(source));
        }
        let (_, _, spans) = PREPARED
            .get()?
            .iter()
            .find(|(name, text, _)| *name == language && text.trim_end() == source.trim_end())?;
        Some(
            spans
                .iter()
                .filter(|(start, end, _)| *end <= source.len() && start < end)
                .map(|(start, end, kind)| (*start..*end, *kind))
                .collect(),
        )
    }
}

impl Installable for NoGrammar {
    fn named(name: &str) -> Option<&'static Self> {
        let _ = name;
        None
    }

    fn resolve(label: &str) -> Option<&'static Self> {
        let _ = label;
        None
    }

    fn status(&self) -> Status {
        match *self {}
    }

    fn start_install(&'static self) {
        match *self {}
    }
}
