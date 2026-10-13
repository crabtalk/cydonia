//! The bubble a person's words sit in: a message in the transcript, and a
//! follow-up in an article's comment thread.

use bezel::{
    gpui::{Div, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
};

/// The bubble, at the width a message takes, on the right of its column.
pub fn bubble(theme: &Theme) -> Div {
    div()
        .self_end()
        .max_w(px(440.))
        .px(px(14.))
        .py(px(9.))
        .rounded(px(Theme::surface_radius()))
        .bg(theme.surface_raised)
        .text_style(TextStyle::Body)
        .text_color(theme.text)
}
