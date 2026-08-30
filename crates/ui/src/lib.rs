//! Widgets cydonia needed and bezel has not got yet.
//!
//! A staging area with an exit: everything here is meant to be proposed to
//! bezel and deleted from this crate once it lands. Nothing may depend on the
//! app — if a widget cannot compile without it, it was never a widget.

use bezel::{
    gpui::{Div, ElementId, Stateful, div, prelude::*, px},
    theme::Theme,
};

/// The `···` that opens a row's menu.
///
/// Three divs rather than an asset: bezel's Solar set has no dots glyph, and a
/// mark this simple is not worth a file — `status_dot` is drawn the same way.
pub fn dots(theme: &Theme) -> Div {
    let tone = theme.text_faint;
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.))
        .children((0..3).map(move |_| div().size(px(3.)).rounded_full().bg(tone)))
}

/// A quiet control: a row of glyph and label that shows a wash on hover and
/// nothing at rest.
///
/// bezel's `Buttons::button` cannot express this — it emits its label as the
/// first child, so anything wanting a mark *before* the text has to build the
/// frame itself, which is how twelve copies of these six lines appeared.
/// Padding and children are the caller's: this owns the shape and the wash.
pub fn ghost(theme: &Theme, id: impl Into<ElementId>) -> Stateful<Div> {
    let wash = theme.glass_hover();
    div()
        .id(id)
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
        .hover(move |el| el.bg(wash))
        .flex()
        .flex_row()
        .items_center()
}
