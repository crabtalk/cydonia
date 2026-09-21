//! Language detection and on-demand WASM syntax highlighting.

use bezel::gpui::App;
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

/// Installed languages. The subset [`highlight`] can answer for right now.
pub fn paintable() -> Vec<&'static str> {
    installed();
    syntax::registry::ready()
}

/// Every language the catalogue can install, which is what the Markdown fence
/// picker lists. Naming one that is not cached is how a fence asks for it —
/// see [`ensure`].
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
pub fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App) {
    installed();
    let mut started = false;
    for label in labels {
        let Some(name) = provider::resolve(&label) else {
            continue;
        };
        if provider::status(name) == Status::Missing {
            provider::start_install(name);
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
pub fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
    installed();
    provider::ensure_runtime().ok()?;
    syntax::highlight(source, language)
}
