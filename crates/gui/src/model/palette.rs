//! The palette the app installs, and everything it is built from.

use crate::model::{
    fonts::Families,
    settings::{self, Paint},
    themes,
};
use bezel::{
    gpui::App,
    theme::{AppExt as _, Appearance, Glass, SurfaceStyle, Theme, ThemeFamily},
};

/// What the palette builder reads. Captured by the builder bezel reruns on
/// every light/dark switch, so a change takes a fresh [`apply`].
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub families: Families,
    pub selection: Option<Paint>,
    pub caret: Option<Paint>,
    /// Terminals take the caret's colour as their cursor.
    pub terminal_caret: bool,
    /// The family each appearance is painted in. `None`, or a family with no
    /// variant for the appearance, is bezel's own palette.
    pub light: Option<&'static ThemeFamily>,
    pub dark: Option<&'static ThemeFamily>,
}

impl Inputs {
    pub fn of(look: &settings::Appearance, families: Families) -> Self {
        Self {
            families,
            selection: look.selection,
            caret: look.caret,
            terminal_caret: look.terminal_caret,
            light: look.theme.light.as_deref().and_then(themes::named),
            dark: look.theme.dark.as_deref().and_then(themes::named),
        }
    }

    fn family(&self, appearance: Appearance) -> Option<&'static ThemeFamily> {
        match appearance {
            Appearance::Light => self.light,
            Appearance::Dark => self.dark,
        }
    }
}

/// The palette for `appearance` under `inputs`.
pub fn build(inputs: &Inputs, appearance: Appearance) -> Theme {
    let mut theme = inputs
        .family(appearance)
        .and_then(|family| family.theme(appearance))
        .unwrap_or_else(|| Theme::for_appearance(appearance));
    theme.drop_preview = SurfaceStyle::Glass(Glass::Clear);
    let families = &inputs.families;
    if let Some(sans) = &families.sans {
        theme.font_body = sans.clone();
        theme.font_sans = sans.clone();
    }
    if let Some(body) = &families.body {
        theme.font_body = body.clone();
    }
    if let Some(mono) = &families.mono {
        theme.font_mono = mono.clone();
    }
    if let Some(color) = inputs.selection {
        theme.selection = color.solid(&theme);
    }
    if let Some(color) = inputs.caret {
        theme.caret = color.solid(&theme);
    }
    if inputs.terminal_caret {
        theme.cursor = theme.caret;
    }
    theme
}

/// Register the builder without installing — for startup, before
/// `appearance::init` installs the first palette.
pub fn register(inputs: Inputs, cx: &mut App) {
    cx.set_palette(move |appearance| build(&inputs, appearance));
}

/// Register the builder and rebuild the installed palette under it.
///
/// `Theme::install` rather than `appearance::apply`, which returns early when
/// the resolved appearance has not moved.
pub fn apply(inputs: Inputs, cx: &mut App) {
    register(inputs, cx);
    let appearance = Theme::of(cx).appearance;
    Theme::install(appearance, cx);
}
