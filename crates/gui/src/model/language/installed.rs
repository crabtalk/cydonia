//! Grammars downloaded on demand from the pinned catalogue — see
//! [`super::provider`].

use super::{Grammars, Installable as _, Language, MARKDOWN, Spec, Status, provider};
use bezel::{gpui::App, theme::HighlightKind};
use std::{ops::Range, path::Path, sync::OnceLock};
use syntax::registry::Known;

/// The catalogue's grammars, as far as they are cached.
pub struct Installed;

impl Grammars for Installed {
    /// The language `path` is written in, or `None` where its name names nothing
    /// the registry holds.
    fn of(path: &Path) -> Option<Language> {
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

    /// Installed languages. The subset [`Grammars::highlight`] can answer for right now.
    fn paintable() -> Vec<&'static str> {
        installed();
        syntax::registry::ready()
    }

    /// Every language the catalogue can install, which is what the Markdown fence
    /// picker lists. Naming one that is not cached is how a fence asks for it —
    /// see [`Grammars::ensure`].
    fn offerable() -> Vec<&'static str> {
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
    fn ensure(labels: impl IntoIterator<Item = String>, cx: &mut App) {
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

    fn highlight(language: &str, source: &str) -> Option<Vec<(Range<usize>, HighlightKind)>> {
        installed();
        provider::ensure_runtime().ok()?;
        syntax::highlight(source, language)
    }
}

/// Register verified cached grammars once, without downloading anything.
pub(super) fn installed() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(provider::initialize);
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
