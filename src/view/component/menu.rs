//! The `···` and `+` menus every sidebar row and column heading opens, and the
//! one field that says which of them is showing.

use crate::view::root::Cydonia;
use bezel::{
    gpui::{self, AnyElement, Context, Div, SharedString, Stateful, Window, div, prelude::*, px},
    theme::Theme,
    ui::menu::{self, Item},
};

/// What a row does when it is picked.
type Act = Box<dyn Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>)>;

/// Which menu is open. One field rather than a flag each, so opening one
/// closes the rest by construction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The `+` on a project heading: what to start here.
    Add(usize),
    /// A project heading: what to do to the project.
    Project(usize),
    /// The `···` on a session row.
    Session(u64),
    /// The `···` on a board row, by project and place in it.
    Board(usize, usize),
    /// The `···` on a table's column heading.
    Column(usize),
}

/// One row of a menu, and what picking it does.
pub(crate) fn row(
    item: Item,
    act: impl Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>) + 'static,
) -> (Item, Act) {
    (item, Box::new(act))
}

impl Cydonia {
    pub(crate) fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
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
    pub(crate) fn menu_card(
        &self,
        id: impl Into<SharedString>,
        rows: Vec<(Item, Act)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (items, acts): (Vec<Item>, Vec<Act>) = rows.into_iter().unzip();
        menu::card(&theme, id, &items, None, cx, move |this, ix, window, cx| {
            this.menu = None;
            acts[ix](this, window, cx);
            cx.notify();
        })
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.menu = None;
            cx.notify();
        }))
        .into_any_element()
    }
}
