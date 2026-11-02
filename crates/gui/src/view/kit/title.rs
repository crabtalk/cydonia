//! The title row over what a pane or the library shows: its mark and name, on
//! a chip while flat, then the tools its view hangs after them.

use crate::view::{
    kit::menu,
    pane::Edge,
    window::{chrome, root},
};
use bezel::{
    gpui::{AnyElement, App, Div, FontWeight, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::tabs::{self, State},
};

/// The hover group a title row is, which its tools show on — the ones other
/// views hang in it included.
pub(crate) const GROUP: &str = "pane-title";

/// Where a title row stands, and whether what it is over holds the focus.
#[derive(Clone, Copy)]
pub(crate) struct Title {
    pub edge: Edge,
    pub floating: bool,
    pub focused: bool,
}

impl Title {
    /// Flat, the chip the name stands on, the focused one lit.
    pub fn chip(self) -> Option<State> {
        (!self.floating).then_some(match self.focused {
            true => State::Focused,
            false => State::Front,
        })
    }

    /// When the row's tools show.
    pub fn reveal(self) -> menu::Reveal {
        match self.chip() {
            Some(State::Focused) => menu::Reveal::Always,
            _ => menu::Reveal::Group(GROUP),
        }
    }

    /// Floating, along the window's top edge it stands on the chrome over the
    /// card; anywhere else it is the card's first row.
    pub fn on_chrome(self) -> bool {
        self.floating && self.edge.top
    }

    /// The row, empty. The card stands in from the window's edges by the
    /// column's padding; a chip's inset and ring slot stand out into it, so
    /// its text lines up with the content under it.
    pub fn row(self, window: &Window, cx: &App) -> Div {
        let corners = chrome::Corners {
            left: self.edge.lights,
            right: self.edge.toggle,
            tabs: false,
        };
        let chip_inset = match self.chip() {
            Some(_) => tabs::TAB_PAD + 1.,
            None => 0.,
        };
        let lead = match corners.left {
            true => corners.lead(0., window, cx) - root::card_gap(self.floating),
            false if self.on_chrome() => root::CARD_EDGE,
            false => root::HEADER_INSET,
        } - chip_inset;
        let trail = match corners.right {
            true => corners.trail(0., window, cx) - root::card_gap(self.floating),
            false => root::CARD_EDGE,
        };
        div()
            .group(GROUP)
            .flex_none()
            .h(px(root::CARD_ROW_HEIGHT))
            .pl(px(lead))
            .pr(px(trail))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
    }

    /// Where the label goes: as wide as it reads along the window's top edge,
    /// where the rest of the row is the window's grip, and the row's free
    /// width anywhere else.
    pub fn slot(self) -> Div {
        div()
            .min_w_0()
            .when(!self.edge.top, |slot| slot.flex_1())
            // A row, so the name is only as wide as it reads — stretched, it
            // would swallow presses on the empty half of the row.
            .flex()
            .flex_row()
            .items_center()
    }

    /// `mark` and `name` on the chip, or bare while floating.
    pub fn label(self, mark: AnyElement, name: impl IntoElement, theme: &Theme) -> Div {
        match self.chip() {
            Some(state) => tabs::chip(theme, state).gap_0(),
            None => div().flex().flex_row().items_center(),
        }
        .min_w_0()
        .child(mark)
        .child(name)
    }

    /// The text of the row's name, for its `child`.
    pub fn name(theme: &Theme) -> Div {
        div()
            .min_w_0()
            .truncate()
            .line_clamp(1)
            .text_style(TextStyle::Subheadline)
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text)
    }
}
