//! The window layout picker: flat or floating, each tile a window in
//! miniature. A pick writes [`Workspace::set_floating`].

use crate::model::workspace::Workspace;
use bezel::{
    gpui::{AnyElement, App, Div, ElementId, Entity, Stateful, div, prelude::*, px},
    theme::Theme,
    ui::widgets::Controls,
};

/// A layout tile's window, and the parts of it drawn to scale.
const LAYOUT_WIDTH: f32 = 112.;
const LAYOUT_HEIGHT: f32 = 72.;
const LAYOUT_SIDEBAR: f32 = 30.;
const LAYOUT_INSET: f32 = 3.;

/// The window layouts the picker offers, in its order: flat, then floating.
const LAYOUTS: [(bool, &str); 2] = [(false, "Flat"), (true, "Floating")];

/// The picker, with the layout `workspace` is set to selected.
pub(crate) fn picker(
    id: impl Into<ElementId>,
    theme: &Theme,
    workspace: &Entity<Workspace>,
    cx: &App,
) -> Stateful<Div> {
    let floating = workspace.read(cx).settings.appearance.floating;
    let tiles = LAYOUTS.map(|(on, name)| (picture(theme, on), name.into()));
    let workspace = workspace.clone();
    theme.picture_picker(id, tiles, usize::from(floating), move |ix, _, cx| {
        let floating = LAYOUTS[*ix].0;
        workspace.update(cx, |workspace, cx| workspace.set_floating(floating, cx));
    })
}

/// A window in miniature, laid out flat or floating, in the colours the window
/// paints.
fn picture(theme: &Theme, floating: bool) -> AnyElement {
    let content = bezel::ui::window::content_fill(floating, theme);
    let bar = |width: f32| {
        div()
            .h(px(3.))
            .w(px(width))
            .rounded_full()
            .bg(theme.text_faint)
    };
    let sidebar = div()
        .flex_none()
        .w(px(LAYOUT_SIDEBAR))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(4.))
        .p(px(6.))
        .when(!floating, |sidebar| {
            sidebar
                .bg(theme.sidebar_bg())
                .border_r_1()
                .border_color(theme.border)
        })
        .children([14., 18., 12.].map(bar));
    let body = div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(4.))
        .p(px(8.))
        .bg(content)
        .children([40., 56., 32.].map(bar));
    let body = match floating {
        true => body
            .m(px(LAYOUT_INSET))
            .rounded(px(LAYOUT_INSET))
            .border_1()
            .border_color(theme.border),
        false => body,
    };
    div()
        .w(px(LAYOUT_WIDTH))
        .h(px(LAYOUT_HEIGHT))
        .flex()
        .flex_row()
        .when(floating, |window| window.bg(theme.sidebar_bg()))
        .child(sidebar)
        .child(body)
        .into_any_element()
}
