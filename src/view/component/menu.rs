//! The `···` and `+` menus every sidebar row and column heading opens, and the
//! one field that says which of them is showing.

use crate::view::root::Cydonia;
use bezel::{
    gpui::{self, AnyElement, Context, Div, SharedString, Stateful, Window, div, prelude::*, px},
    motion::{Fade, Painter},
    theme::Theme,
    ui::{icons, popover},
};

/// Which menu is open. One field rather than a flag each, so opening one
/// closes the rest by construction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The `+` on a project heading: what to start here.
    Add(usize),
    /// The `···` on a project heading: what to do to the project.
    Project(usize),
    /// The `···` on a session row.
    Session(u64),
    /// The `···` on a board row, by project and place in it.
    Board(usize, usize),
    /// The `···` on a table's column heading.
    Column(usize),
}

impl Cydonia {
    fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        cx.notify();
    }

    /// A `···` or `+` that opens `menu`, revealed on the row's hover.
    pub(crate) fn menu_button(
        &self,
        id: impl Into<gpui::ElementId>,
        group: &'static str,
        mark: impl IntoElement,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id.into())
            .flex_none()
            .relative()
            // An open menu keeps its trigger on show — by then the pointer is
            // over the menu, not the row that opened it.
            .when(self.menu != Some(menu), |el| {
                el.invisible().group_hover(group, |el| el.visible())
            })
            .rounded(px(Theme::control_radius()))
            .p(px(3.))
            .cursor_pointer()
            .child(mark)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(menu, cx);
            }))
    }

    /// The card every sidebar menu hangs in, dismissed by a press outside it.
    pub(crate) fn menu_card(&self, rows: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        popover::popover_card(&theme)
            .w(px(170.))
            .child(div().flex().flex_col().children(rows))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn menu_row(
        &self,
        key: impl Into<SharedString>,
        glyph: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let key = key.into();
        popover::menu_row(&theme, false, Some(Fade::new(painter, key.clone())))
            .id(key)
            .child(
                icons::icon(glyph)
                    .size(px(13.))
                    .flex_none()
                    .text_color(theme.text_faint),
            )
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.menu = None;
                act(this, window, cx);
                cx.notify();
            }))
            .into_any_element()
    }
}
