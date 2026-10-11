//! Language detection and on-demand WASM syntax highlighting.
//!
//! Without the `desktop` feature there are no grammars: nothing is installed,
//! only Markdown is recognised, and a fence is painted only when its spans
//! were worked out ahead of time and handed to [`prepare`].

#[cfg(feature = "desktop")]
use bezel::gpui::App;
#[cfg(feature = "desktop")]
use std::sync::OnceLock;
use std::{ops::Range, path::Path};

use bezel::theme::HighlightKind;
#[cfg(feature = "desktop")]
use syntax::registry::Known;

#[cfg(feature = "desktop")]
mod provider;
#[cfg(feature = "desktop")]
pub use provider::Spec;

/// The registry names markdown and carries no grammar for it; `markdown`
/// paints it from its own parser.
const MARKDOWN: &str = "markdown";

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

/// Register verified cached grammars once, without downloading anything.
#[cfg(feature = "desktop")]
fn installed() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(provider::initialize);
}

/// The language `path` is written in, or `None` where its name names nothing
/// the registry holds.
#[cfg(feature = "desktop")]
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

/// Installed languages. The subset [`highlight`] can answer for right now.
#[cfg(feature = "desktop")]
pub fn paintable() -> Vec<&'static str> {
    installed();
    syntax::registry::ready()
}

/// Every language the catalogue can install, which is what the Markdown fence
/// picker lists. Naming one that is not cached is how a fence asks for it —
/// see [`ensure`].
#[cfg(feature = "desktop")]
pub fn offerable() -> Vec<&'static str> {
    installed();
    provider::names()
}

/// Fetch the grammars `labels` name, and keep the window repainting until they
/// land. A fence tagged with a language is the whole of the request: writing
/// one is not an occasion to ask a person to go and install something.
///
/// Only [`Status::Missing`] is acted on. A download that failed is left where
/// it stopped rather than retried on the next keystroke; retrying is still the
/// file view's Retry action.
#[cfg(feature = "desktop")]
pub fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App) {
    installed();
    let mut started = false;
    for label in labels {
        let Some(spec) = Spec::resolve(&label) else {
            continue;
        };
        if spec.status() == Status::Missing {
            spec.start_install();
            started = true;
        }
    }
    if started {
        watch(cx);
    }
}

/// Repaint while a download runs, so a fence colours itself the moment its
/// grammar arrives.
///
/// Nothing is re-registered here. [`offerable`] is the whole catalogue and
/// does not grow, and [`highlight`] resolves through the registry on every
/// call — so what a finished download changes is what the next frame paints,
/// and a frame is the only thing that has to be asked for.
#[cfg(feature = "desktop")]
fn watch(cx: &mut App) {
    if !provider::begin_watch() {
        return;
    }
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let busy = cx.update(|cx| {
                cx.refresh_windows();
                provider::working()
            });
            if !busy {
                break;
            }
        }
        provider::end_watch();
    })
    .detach();
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
#[cfg(feature = "desktop")]
pub fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    installed();
    provider::ensure_runtime().ok()?;
    syntax::highlight(source, language)
}

#[cfg(not(feature = "desktop"))]
mod none {
    use super::{HighlightKind, Language, MARKDOWN, Status};
    use bezel::gpui::App;
    use std::{ops::Range, path::Path, sync::OnceLock};

    /// A fence painted ahead of time: its language, its source, and the spans
    /// as byte offsets into that source.
    pub type Prepared = (
        &'static str,
        &'static str,
        &'static [(usize, usize, HighlightKind)],
    );

    static PREPARED: OnceLock<&'static [Prepared]> = OnceLock::new();

    /// The fences this build can paint. Set once; a second call is ignored.
    pub fn prepare(table: &'static [Prepared]) {
        let _ = PREPARED.set(table);
    }

    pub fn of(path: &Path) -> Option<Language> {
        let ext = path.extension()?.to_str()?;
        matches!(ext, "md" | "markdown").then_some(Language::Markdown)
    }

    pub fn paintable() -> Vec<&'static str> {
        Vec::new()
    }

    pub fn offerable() -> Vec<&'static str> {
        vec![MARKDOWN]
    }

    pub fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App) {
        let _ = (labels.into_iter(), cx);
    }

    /// Matched on the source with trailing whitespace ignored, which is the
    /// one way a fence's body and the file it was read from differ.
    pub fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
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

    /// No grammar is in the catalogue without the `desktop` feature.
    pub enum Spec {}

    impl Spec {
        pub fn named(name: &str) -> Option<&'static Self> {
            let _ = name;
            None
        }

        pub fn resolve(label: &str) -> Option<&'static Self> {
            let _ = label;
            None
        }

        pub fn status(&self) -> Status {
            match *self {}
        }

        pub fn start_install(&'static self) {
            match *self {}
        }
    }
}

#[cfg(not(feature = "desktop"))]
pub use none::{Prepared, Spec, ensure, highlight, of, offerable, paintable, prepare};
