//! File and terminal zoom are independent and temporary. Reset uses the saved base size.

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
            base: crate::model::settings::MONO_TEXT_SIZE,
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

#[derive(Clone, Copy)]
struct FileFont(TerminalFont);

impl Default for FileFont {
    fn default() -> Self {
        Self(TerminalFont {
            base: bezel::theme::TextStyle::Body.size(),
            adjustment: 0.,
        })
    }
}

impl Global for FileFont {}

fn file_font(cx: &App) -> TerminalFont {
    cx.try_global::<FileFont>().copied().unwrap_or_default().0
}

pub fn file_size(cx: &App) -> f32 {
    file_font(cx).size()
}

fn update_file(cx: &mut App, update: impl FnOnce(&mut TerminalFont)) {
    let mut font = file_font(cx);
    update(&mut font);
    cx.set_global(FileFont(font));
    cx.refresh_windows();
}

pub fn set_file_size(points: f32, cx: &mut App) {
    update_file(cx, |font| font.rebase(points));
}

pub fn zoom_file(by: f32, cx: &mut App) {
    update_file(cx, |font| font.step(by));
}

pub fn reset_file_zoom(cx: &mut App) {
    update_file(cx, |font| font.adjustment = 0.);
}

#[cfg(test)]
#[path = "../../tests/unit/typography.rs"]
mod tests;
