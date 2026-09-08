//! A menu row that opens a card beside itself — the shape AppKit and SwiftUI
//! give a submenu.
//!
//! bezel has none: `menu::Item` is `Action | Separator`, so `menu::card` cannot
//! give a row a trailing glyph or a card of its own, and every
//! `popover::anchored_*` helper opens above or below a trigger rather than off
//! a row's trailing edge. This is the smallest thing that behaves like one,
//! assembled from the primitives bezel does expose.
//!
//! Written generic over the view, as `menu::card` is, so lifting it upstream is
//! a move rather than a rewrite. See [`super`].

use bezel::{
    gpui::{
        self, Anchor, AnyElement, Context, ElementId, MouseButton, SharedString, div, prelude::*,
        px, svg,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{icons, popover, surface},
};
use std::rc::Rc;

/// A row's glyph, and the width the rows without one keep for it. bezel's own
/// is private to its menu module, and a menu of mixed rows has to keep its
/// labels on one edge either way.
const GLYPH: f32 = 13.;

/// How wide a card sits, matching `menu::card`.
const WIDTH: f32 = 180.;

/// A card's inner inset, so a submenu's first row can be pulled back up onto
/// the row that opened it. bezel's own is `pub(crate)`.
const PAD: f32 = 4.;

/// The gap between a row and the card it opens.
const GAP: f32 = 4.;

/// What a row is currently set to, shown before its chevron. Carries a glyph
/// of its own because what a row is *on* can be a thing with a mark — an agent,
/// a provider — and the name alone would be the lesser half of it.
#[derive(Clone)]
pub struct Value {
    pub icon: Option<SharedString>,
    pub label: SharedString,
}

impl Value {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            icon: None,
            label: label.into(),
        }
    }

    pub fn with_icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A menu card: bezel's popover card at the width its menus use.
pub fn card(theme: &Theme) -> gpui::Div {
    popover::popover_card(theme).min_w(px(WIDTH))
}

/// One row, in bezel's own shape: its glyph, the label, then the check when it
/// is the one in use.
///
/// A row with no glyph keeps no room for one. `menu::card` decides that across
/// the whole card, so a mixed card keeps its labels on one edge; this one is
/// built a row at a time and cannot see its siblings, so a card mixing the two
/// should pass every row an icon.
///
/// Returned bare so the caller hangs its own `on_click` on it — what a row
/// *does* is never this component's business.
pub fn row<V: 'static>(
    id: impl Into<ElementId>,
    label: SharedString,
    icon: Option<SharedString>,
    checked: bool,
    theme: &Theme,
    cx: &mut Context<V>,
) -> gpui::Stateful<gpui::Div> {
    let id = id.into();
    // Keyed off the id so the hover fade is the same one frame to frame; a key
    // that moved would restart the blend on every repaint.
    let fade = Fade::new(Painter::of(cx), SharedString::from(format!("{id:?}")));
    popover::menu_row(theme, false, Some(fade))
        .id(id)
        .children(icon.map(|icon| glyph_slot(theme, icon)))
        .child(div().flex_1().min_w_0().child(label))
        .when(checked, |row| {
            row.child(
                icons::icon(icons::status::CHECK)
                    .size(px(GLYPH))
                    .text_color(theme.text),
            )
        })
}

/// A row that opens `card` beside itself: its name, the value it is on, and the
/// chevron that says there is more behind it.
///
/// `open` is the caller's — one menu has at most one submenu out, and which one
/// is state the menu owns rather than the row. `set_open` is called with what
/// it should become: hovering asks for `true`, and a press toggles, so a menu
/// is reachable without a pointer.
///
/// Hovering *away* is deliberately not an answer. The card is a deferred layer
/// and not geometrically inside the row, so a pointer travelling into it leaves
/// the row and would close the thing it was reaching for. Another row opening
/// is what closes this one, which is what AppKit does anyway.
#[expect(
    clippy::too_many_arguments,
    reason = "one row, and it has that many parts"
)]
pub fn opener<V: 'static>(
    id: impl Into<ElementId>,
    label: SharedString,
    value: Option<Value>,
    icon: Option<SharedString>,
    open: bool,
    card: gpui::Div,
    theme: &Theme,
    cx: &mut Context<V>,
    set_open: impl Fn(&mut V, bool, &mut Context<V>) + 'static,
) -> AnyElement {
    let set_open = Rc::new(set_open);
    let hover = set_open.clone();
    row(id, label, icon, false, theme, cx)
        // The row is the card's origin, so it has to be the box the
        // absolutely-placed layer measures from.
        .relative()
        // The value before the chevron — the reason to open a menu like this is
        // as often to read what it is on as to change it.
        .children(value.map(|value| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(5.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .children(value.icon.map(|path| {
                    svg()
                        .path(path)
                        .size(px(GLYPH))
                        .flex_none()
                        .text_color(theme.text_faint)
                }))
                .child(value.label)
        }))
        .child(
            icons::icon(icons::arrows::ALT_ARROW_RIGHT)
                .size(px(GLYPH))
                .text_color(if open { theme.text } else { theme.text_faint }),
        )
        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
            if *hovered && !open {
                hover(view, true, cx);
            }
        }))
        .on_click(cx.listener(move |view, _, _, cx| set_open(view, !open, cx)))
        .children(open.then(|| layer(card)))
        .into_any_element()
}

/// The floating layer that puts a card off the row's trailing edge.
fn layer(card: gpui::Div) -> AnyElement {
    // Zero-size and pinned to the corner, for the reason bezel pins its own
    // layers that way: without it the anchored box takes its static position
    // from the row's `items_center` and lands halfway down.
    //
    // Deferred above the card holding it so it paints over rather than under,
    // and it swallows its own presses — a menu dismisses on a press outside its
    // card, which this sits outside of.
    div()
        .absolute()
        .top_0()
        .right_0()
        .size_0()
        .child(
            gpui::deferred(
                gpui::anchored()
                    .anchor(Anchor::TopLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .occlude()
                            .pl(px(GAP))
                            // Back up by the card's own inset, so the first row
                            // sits on the row that opened it rather than a
                            // padding's worth below it.
                            .mt(px(-PAD))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            // The fill, the hairline and the shadow are the
                            // surface's, not the card's: `popover_card` paints
                            // none of them where the theme is glass, which is
                            // why bezel wraps its own menus in this.
                            .child(surface::popover(Theme::surface_radius(), card)),
                    ),
            )
            .priority(2),
        )
        .into_any_element()
}

/// A row's leading column, sized so a card of glyphed rows keeps its labels on
/// one edge whatever each glyph's own aspect is.
fn glyph_slot(theme: &Theme, icon: SharedString) -> gpui::Div {
    div()
        .flex_none()
        .size(px(GLYPH))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(icon)
                .size(px(GLYPH))
                .text_color(theme.text_faint),
        )
}
