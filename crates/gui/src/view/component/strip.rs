//! A tab strip with a scrollbar over it rather than beside it, so the bar
//! takes no room from the row it sits in.

use bezel::{
    gpui::{App, Axis, Div, ScrollHandle, SharedString, Stateful, Window, div, prelude::*},
    ui::scroll as scrollbars,
};

/// `bar` — a [`bezel::ui::tabs::bar`] — tracked by a handle kept per `key`,
/// with a horizontal overlay bar on top.
pub fn strip(
    key: impl Into<SharedString>,
    bar: Stateful<Div>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let key = key.into();
    let handle = window
        .use_keyed_state(
            SharedString::from(format!("strip-scroll-{key}")),
            cx,
            |_, _| ScrollHandle::new(),
        )
        .read(cx)
        .clone();
    div()
        .relative()
        .min_w_0()
        .flex()
        .child(bar.track_scroll(&handle))
        .child(scrollbars::Overlay::new(
            SharedString::from(format!("strip-bar-{key}")),
            &handle,
            Axis::Horizontal,
        ))
}
