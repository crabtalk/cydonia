//! The families the app is set in, and the list of families to pick from.
//!
//! Three answers rather than one per surface: a family reaches the app through
//! `Theme::font_sans`, `Theme::font_body` and `Theme::font_mono`, and every
//! surface paints from one of those three — chrome from the first, a rendered
//! document from the second, the terminal grid and code from the third. The
//! palette builder in [`crate::model::palette`] writes them.

use crate::model::settings;
use bezel::gpui::{App, Pixels, SharedString, font, px};
use std::sync::{
    RwLock,
    atomic::{AtomicBool, Ordering},
};

/// What the reader picked, or nothing where they have not. Nothing keeps the
/// palette's own faces, which differ per platform.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Families {
    pub sans: Option<SharedString>,
    /// Unset follows [`Self::sans`], which is what an interface family picked
    /// on its own has to mean for the prose inside it.
    pub body: Option<SharedString>,
    pub mono: Option<SharedString>,
}

impl Families {
    pub fn of(look: &settings::Appearance) -> Self {
        Self {
            sans: look.ui_font.clone().map(Into::into),
            body: look.article_font.clone().map(Into::into),
            mono: look.mono_font.clone().map(Into::into),
        }
    }
}

/// Whether terminals draw the app's caret's shape and blink. Read by the
/// terminal view, which has no workspace to ask.
static TERMINAL_CARET: AtomicBool = AtomicBool::new(false);

pub fn terminal_caret() -> bool {
    TERMINAL_CARET.load(Ordering::Relaxed)
}

pub fn set_terminal_caret(on: bool) {
    TERMINAL_CARET.store(on, Ordering::Relaxed);
}

/// A family the system can set text in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub name: SharedString,
    /// Whether every glyph in it takes the same advance, measured rather than
    /// declared: the platform list carries no such flag.
    pub mono: bool,
}

/// The size the probe measures at. Any size answers the question — advances
/// scale linearly — and a large one keeps two near-equal widths apart.
const PROBE: Pixels = px(64.);

/// Measured once per process — the set of installed families does not move
/// under a running app often enough to pay for it on every settings window.
static INSTALLED: RwLock<Option<Vec<Family>>> = RwLock::new(None);

/// Every family installed on this machine, mono ones marked.
///
/// Names beginning with a dot are the platform's private aliases — the two
/// bezel's palette is built on among them — and name nothing a reader would
/// recognise, so they are left out and "system" stands for them in the picker.
///
/// Each family costs a font load and three glyph measurements, so this is for
/// the settings window opening and not for a frame.
pub fn installed(cx: &App) -> Vec<Family> {
    if let Some(measured) = INSTALLED.read().ok().and_then(|held| held.clone()) {
        return measured;
    }
    let text_system = cx.text_system();
    let measured: Vec<Family> = text_system
        .all_font_names()
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .map(|name| {
            let id = text_system.resolve_font(&font(name.clone()));
            let advance = |ch| text_system.advance(id, PROBE, ch).map(|size| size.width);
            let mono = match (advance('i'), advance('M'), advance('0')) {
                (Ok(i), Ok(m), Ok(zero)) => i == m && m == zero,
                _ => false,
            };
            Family {
                name: name.into(),
                mono,
            }
        })
        .collect();
    if let Ok(mut held) = INSTALLED.write() {
        *held = Some(measured.clone());
    }
    measured
}
