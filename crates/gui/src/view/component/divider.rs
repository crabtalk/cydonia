//! Shared pane seam with a wide, invisible resize target.

use bezel::{
    gpui::{Axis, Div, div, prelude::*, px},
    theme::Theme,
};

pub const HIT: f32 = 9.;

pub fn divider(theme: &Theme, axis: Axis) -> Div {
    let handle = div()
        .group("pane-divider")
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    let line = div().bg(theme.border);
    match axis {
        Axis::Horizontal => handle.w(px(HIT)).h_full().cursor_col_resize().child(
            line.w(px(1.)).h_full().group_hover("pane-divider", |line| {
                line.w(px(2.)).bg(theme.text_muted.opacity(0.45))
            }),
        ),
        Axis::Vertical => handle.h(px(HIT)).w_full().cursor_row_resize().child(
            line.h(px(1.)).w_full().group_hover("pane-divider", |line| {
                line.h(px(2.)).bg(theme.text_muted.opacity(0.45))
            }),
        ),
    }
}
