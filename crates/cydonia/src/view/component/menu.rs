//! The `···` and `+` menus every sidebar row and column heading opens, and the
//! one field that says which of them is showing.

use crate::view::{root::Cydonia, sidebar::Row};
use bezel::{
    gpui::{self, AnyElement, Context, Div, SharedString, Stateful, Window, prelude::*, px},
    theme::Theme,
    ui::{
        menu::{self, Hit, Item},
        widgets::Buttons,
    },
};

/// What a row does when it is picked.
type Act = Box<dyn Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>)>;

/// Which menu is open. One field rather than a flag each, so opening one
/// closes the rest by construction.
///
/// Cloned rather than copied since [`Menu::Card`] names its card, which is a
/// string — a card is addressed by id everywhere else and a menu key that used
/// its position would open the wrong one the moment a card moved.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The `+` on a project heading: what to start here.
    Add(usize),
    /// A project heading: what to do to the project.
    Project(usize),
    /// The `···` on an entry's row, whichever kind it is.
    Entry(Row),
    /// The `···` in the pane header. Its own key rather than `Entry` of what
    /// the header is showing: that entry has a row in the sidebar too, and a
    /// key naming the entry would have one click open both of them.
    Header,
    /// The kind picker above the projects.
    Filter,
    /// The `···` on a table's column heading.
    Column(usize),
    /// The `···` on a card, by card id.
    Card(String),
}

/// One row of a menu, and what picking it does.
pub(crate) fn row(
    item: Item,
    act: impl Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>) + 'static,
) -> (Item, Act) {
    (item, Box::new(act))
}

impl Cydonia {
    /// Open a menu, or shut the one already open.
    ///
    /// The press that reaches a trigger is the same press the open card
    /// dismisses on, so by click time the menu already reads as shut and a
    /// plain toggle would open it straight back. What the press found is noted
    /// by [`Cydonia::menu_press`] instead, in the capture phase — ahead of
    /// that handler, whichever element owns it.
    pub(crate) fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        let closed_by_this_press = std::mem::take(&mut self.menu_pressed);
        let shut = closed_by_this_press || self.menu.as_ref() == Some(&menu);
        self.menu = (!shut).then_some(menu);
        self.menu_cursor.clear();
        cx.notify();
    }

    /// Shut whichever menu is open, and forget the row it was on.
    fn shut_menu(&mut self) {
        self.menu = None;
        self.menu_cursor.clear();
    }

    /// Note, on the way down, whether the press landed on the trigger of the
    /// menu that is open. Every trigger claims this, so the note is written
    /// afresh on each press and can never be read stale.
    pub(crate) fn menu_press(
        &self,
        el: Stateful<Div>,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        el.capture_any_mouse_down(cx.listener(move |this, _, _, _| {
            this.menu_pressed = this.menu.as_ref() == Some(&menu);
        }))
    }

    /// A `···` or `+` that opens `menu`, revealed on the row's hover.
    /// `group` is the hover group that reveals it — one row in a list of them,
    /// where a `···` on every line at once would be noise. `None` for a trigger
    /// that is the only one on screen and stands on its own, which is what the
    /// pane header's is.
    pub(crate) fn menu_button(
        &self,
        id: impl Into<gpui::ElementId>,
        group: Option<&'static str>,
        mark: impl IntoElement,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let button = Theme::of(cx)
            .ghost(id)
            .flex_none()
            .relative()
            // An open menu keeps its trigger on show — by then the pointer is
            // over the menu, not the row that opened it.
            .when_some(
                group.filter(|_| self.menu.as_ref() != Some(&menu)),
                |el, group| el.invisible().group_hover(group, |el| el.visible()),
            )
            .p(px(3.))
            .child(mark)
            .on_click({
                let menu = menu.clone();
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_menu(menu.clone(), cx);
                })
            });
        self.menu_press(button, menu, cx)
    }

    /// The card every sidebar menu hangs in, dismissed by a press outside it —
    /// which the card reports itself, since with a panel open only the tree
    /// knows which presses landed on none of it.
    pub(crate) fn menu_card(
        &self,
        id: impl Into<SharedString>,
        rows: Vec<(Item, Act)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (items, acts): (Vec<Item>, Vec<Act>) = rows.into_iter().unzip();
        // A hit names a row by its path, and reading one back means holding
        // the list it was built from.
        let paths = items.clone();
        menu::card(
            &theme,
            id,
            &items,
            &self.menu_cursor,
            cx,
            move |this, hit, window, cx| match hit {
                Hit::Point(path) => {
                    if this.menu_cursor.point_at(&paths, &path) {
                        cx.notify();
                    }
                }
                Hit::Choose(path) => {
                    let [row] = path[..] else { return };
                    this.shut_menu();
                    if let Some(act) = acts.get(row) {
                        act(this, window, cx);
                    }
                    cx.notify();
                }
                Hit::Dismiss => {
                    this.shut_menu();
                    cx.notify();
                }
            },
        )
        .into_any_element()
    }
}
