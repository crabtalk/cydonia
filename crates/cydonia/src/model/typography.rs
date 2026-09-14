//! Terminal zoom is shared by sessions, independent of article zoom, and never
//! written into settings. Reset returns to the configured base size.

use super::settings::clamp_content_text_size;
use bezel::gpui::{App, Global};

#[derive(Clone, Copy)]
struct TerminalFont {
    base: f32,
    adjustment: f32,
}

impl Default for TerminalFont {
    fn default() -> Self {
        Self {
            base: terminal::view::TERM_FONT_SIZE,
            adjustment: 0.,
        }
    }
}

impl Global for TerminalFont {}

impl TerminalFont {
    fn of(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    fn size(self) -> f32 {
        clamp_content_text_size(self.base + self.adjustment)
    }

    fn step(&mut self, by: f32) {
        // Clamp before storing so repeated presses at a limit do not build up
        // invisible zoom that must be unwound before the next press responds.
        self.adjustment = clamp_content_text_size(self.size() + by) - self.base;
    }

    fn rebase(&mut self, points: f32) {
        self.base = clamp_content_text_size(points);
        self.adjustment = self.size() - self.base;
    }

    fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.refresh_windows();
    }
}

pub fn terminal_size(cx: &App) -> f32 {
    TerminalFont::of(cx).size()
}

pub fn set_terminal_size(points: f32, cx: &mut App) {
    let mut font = TerminalFont::of(cx);
    font.rebase(points);
    font.install(cx);
}

pub fn zoom_terminal(by: f32, cx: &mut App) {
    let mut font = TerminalFont::of(cx);
    font.step(by);
    font.install(cx);
}

pub fn reset_terminal_zoom(cx: &mut App) {
    let mut font = TerminalFont::of(cx);
    font.adjustment = 0.;
    font.install(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::settings::CONTENT_TEXT_SIZE;

    #[test]
    fn zoom_preserves_base_and_follows_a_changed_default() {
        let mut font = TerminalFont::default();
        font.step(1.);
        font.step(1.);
        assert_eq!(font.base, 13.);
        assert_eq!(font.size(), 15.);
        font.rebase(16.);
        assert_eq!(font.size(), 18.);
        font.adjustment = 0.;
        assert_eq!(font.size(), 16.);
    }

    #[test]
    fn zoom_responds_immediately_after_hitting_either_limit() {
        let mut font = TerminalFont::default();
        for _ in 0..100 {
            font.step(1.);
        }
        assert_eq!(font.size(), CONTENT_TEXT_SIZE.1);
        font.step(-1.);
        assert_eq!(font.size(), CONTENT_TEXT_SIZE.1 - 1.);
        for _ in 0..100 {
            font.step(-1.);
        }
        assert_eq!(font.size(), CONTENT_TEXT_SIZE.0);
        font.step(1.);
        assert_eq!(font.size(), CONTENT_TEXT_SIZE.0 + 1.);
    }
}
