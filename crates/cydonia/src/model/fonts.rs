//! The families the app is set in, and the list of families to pick from.
//!
//! Three answers rather than one per surface: a family reaches the app through
//! `Theme::font_sans`, `Theme::font_body` and `Theme::font_mono`, and every
//! surface paints from one of those three — chrome from the first, a rendered
//! document from the second, the terminal grid and code from the third.
//!
//! The chosen families live in a process-wide static rather than a gpui global
//! because bezel takes the palette builder as a bare `fn(Appearance) -> Theme`
//! ([`theme::set_palette`]) and hands it no context to read one from.

use bezel::{
    gpui::{App, Pixels, SharedString, font, px},
    theme::{Appearance, Theme},
};
use std::sync::RwLock;

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

static FAMILIES: RwLock<Option<Families>> = RwLock::new(None);

fn held() -> Families {
    FAMILIES
        .read()
        .ok()
        .and_then(|held| held.clone())
        .unwrap_or_default()
}

/// The families the app is currently set in.
pub fn families() -> Families {
    held()
}

/// The palette builder to register with [`bezel::theme::set_palette`] before
/// `appearance::init`. Registered rather than installed once: bezel rebuilds
/// the palette from scratch on every light/dark switch, and a family written
/// straight onto the theme would last until sunset.
pub fn palette(appearance: Appearance) -> Theme {
    let mut theme = Theme::for_appearance(appearance);
    let families = held();
    if let Some(sans) = families.sans {
        theme.font_body = sans.clone();
        theme.font_sans = sans;
    }
    if let Some(body) = families.body {
        theme.font_body = body;
    }
    if let Some(mono) = families.mono {
        theme.font_mono = mono;
    }
    theme
}

/// Record the families without repainting — for startup, before the first
/// palette is installed.
pub fn init(families: Families) {
    if let Ok(mut held) = FAMILIES.write() {
        *held = Some(families);
    }
}

/// Record the families and rebuild the palette under them.
///
/// `Theme::install` rather than `appearance::apply`, which returns early when
/// the resolved appearance has not moved — and it never has here.
pub fn set(families: Families, cx: &mut App) {
    init(families);
    let appearance = Theme::of(cx).appearance;
    Theme::install(appearance, cx);
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

#[cfg(test)]
#[path = "../../tests/unit/fonts.rs"]
mod tests;
