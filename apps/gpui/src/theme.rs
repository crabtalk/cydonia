//! Flat color tokens in a global. Numbers drive layout, colors are paint.

use gpui::{App, Global, Hsla, hsla};

#[derive(Clone, Copy)]
pub struct Theme {
    pub canvas: Hsla,
    pub sidebar: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub composer: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub accent: Hsla,
    pub success: Hsla,
    pub danger: Hsla,
    pub code_wash: Hsla,
    pub selection: Hsla,
}

struct ActiveTheme(Theme);

impl Global for ActiveTheme {}

impl Theme {
    pub fn dark() -> Self {
        Self {
            canvas: hsla(0., 0., 0.09, 1.),
            sidebar: hsla(0., 0., 0.07, 1.),
            surface: hsla(0., 0., 0.10, 1.),
            raised: hsla(0., 0., 0.13, 1.),
            composer: hsla(0., 0., 0.13, 1.),
            border: hsla(0., 0., 1., 0.06),
            text: hsla(0., 0., 0.92, 1.),
            text_secondary: hsla(0., 0., 0.60, 1.),
            text_tertiary: hsla(0., 0., 0.40, 1.),
            accent: hsla(14. / 360., 0.65, 0.60, 1.),
            success: hsla(145. / 360., 0.45, 0.50, 1.),
            danger: hsla(0., 0.55, 0.55, 1.),
            code_wash: hsla(0., 0., 1., 0.04),
            selection: hsla(14. / 360., 0.65, 0.60, 0.25),
        }
    }

    /// Designed over the same tokens, not inverted: surfaces go light,
    /// the accent drops to a darker level for contrast on white.
    pub fn light() -> Self {
        Self {
            canvas: hsla(0., 0., 0.97, 1.),
            sidebar: hsla(0., 0., 0.94, 1.),
            surface: hsla(0., 0., 0.98, 1.),
            raised: hsla(0., 0., 1., 1.),
            composer: hsla(0., 0., 1., 1.),
            border: hsla(0., 0., 0., 0.08),
            text: hsla(0., 0., 0.15, 1.),
            text_secondary: hsla(0., 0., 0.40, 1.),
            text_tertiary: hsla(0., 0., 0.55, 1.),
            accent: hsla(14. / 360., 0.70, 0.45, 1.),
            success: hsla(145. / 360., 0.50, 0.35, 1.),
            danger: hsla(0., 0.60, 0.45, 1.),
            code_wash: hsla(0., 0., 0., 0.04),
            selection: hsla(14. / 360., 0.70, 0.45, 0.25),
        }
    }

    pub fn of(cx: &App) -> Theme {
        cx.global::<ActiveTheme>().0
    }
}

pub fn init(cx: &mut App) {
    use gpui::WindowAppearance;
    let theme = match cx.window_appearance() {
        WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::light(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
    };
    cx.set_global(ActiveTheme(theme));
}
