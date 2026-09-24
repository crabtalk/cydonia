//! The `···` and `+` menus every sidebar row and column heading opens, and the
//! one field that says which of them is showing.

use crate::view::{root::Cydonia, sidebar::Row};
use bezel::{
    gpui::{
        self, AnyElement, Context, Div, Pixels, Point, SharedString, Stateful, Window, prelude::*,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        icons::Icon,
        menu::{self, Hit, Item},
        widgets::{ButtonStyle, Buttons},
    },
};

/// What a row does when it is picked, handed the rest of the path — which of a
/// submenu's rows it was, and empty for a row that opens nothing.
pub(crate) type Act = Box<dyn Fn(&mut Cydonia, &[usize], &mut Window, &mut Context<Cydonia>)>;

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
    /// The `···` in one pane's bar, by the entry the pane is on. A space has
    /// several bars on screen at once, so `Header` alone would open every one
    /// of them together.
    Pane(gpui::SharedString),
    /// The `···` on a table's column heading.
    Column(usize),
    /// The `···` on a board's lane, by column id. An id and not a position: a
    /// lane's whole menu is about moving it, and a key that moved with it would
    /// shut the menu on every press.
    Lane(String),
    /// The agents on offer under `New session`, on the screen a project with
    /// nothing open shows — see [`crate::view::root::Cydonia::launch`].
    Launch,
    /// The block picker on the article ribbon — see
    /// [`crate::view::component::ribbon`].
    Turn,
    /// The `···` on a card, by card id.
    Card(String),
    /// The menu bar's tree, off macOS — see [`crate::view::menubar::button`].
    App,
}

/// One row of a menu, and what picking it does.
pub(crate) fn row(
    item: Item,
    act: impl Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>) + 'static,
) -> (Item, Act) {
    (
        item,
        Box::new(move |this, _, window, cx| act(this, window, cx)),
    )
}

/// A row that drops a panel of rows of its own. Picking one of those is what
/// acts; the submenu row itself only opens.
pub(crate) fn submenu(
    label: impl Into<SharedString>,
    icon: impl Into<Icon>,
    rows: Vec<(Item, Act)>,
) -> (Item, Act) {
    let (items, acts): (Vec<Item>, Vec<Act>) = rows.into_iter().unzip();
    let item = Item::submenu(label, items).with_icon(icon);
    let act: Act = Box::new(move |this, path, window, cx| {
        if let Some((&at, rest)) = path.split_first()
            && let Some(act) = acts.get(at)
        {
            act(this, rest, window, cx);
        }
    });
    (item, act)
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
        self.toggle_menu_at(menu, None, cx);
    }

    /// [`Cydonia::toggle_menu`] for a press with a point to it: `at` is where
    /// the card is to stand, in window space, and the trigger's own edge is
    /// what it falls back to. Read by [`Cydonia::menu_point`].
    pub(crate) fn toggle_menu_at(
        &mut self,
        menu: Menu,
        at: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let closed_by_this_press = std::mem::take(&mut self.menu_pressed);
        let shut = closed_by_this_press || self.menu.as_ref() == Some(&menu);
        self.menu = (!shut).then_some(menu);
        self.menu_point = (!shut).then_some(at).flatten();
        self.menu_cursor.clear();
        cx.notify();
    }

    /// Where the open menu was pressed, for a card that is to stand there
    /// rather than on its trigger. `None` once `menu` is not the open one, so
    /// a card built for another row never reads this.
    pub(crate) fn menu_point(&self, menu: &Menu) -> Option<Point<Pixels>> {
        (self.menu.as_ref() == Some(menu))
            .then_some(self.menu_point)
            .flatten()
    }

    /// Shut whichever menu is open, and forget the row it was on.
    fn shut_menu(&mut self) {
        self.menu = None;
        self.menu_point = None;
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
        id: impl Into<SharedString>,
        group: Option<&'static str>,
        mark: impl Into<Icon>,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let id = id.into();
        // The glyph is built here rather than taken, the way
        // `Buttons::icon_button` builds its own: a trigger's mark is the
        // trigger's metric, not the caller's.
        let button = Theme::of(cx)
            .icon_button(
                mark,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), id.clone())),
            )
            .id(id)
            .flex_none()
            .relative()
            // An open menu keeps its trigger on show — by then the pointer is
            // over the menu, not the row that opened it.
            .when_some(
                group.filter(|_| self.menu.as_ref() != Some(&menu)),
                |el, group| {
                    if matches!(menu, Menu::Add(_) | Menu::Entry(_)) {
                        // Resolve space during render, never in a hover style:
                        // GPUI can resolve hover differently in prepaint and paint.
                        el.when(self.sidebar_hovered.as_ref() != Some(&menu), |el| {
                            el.hidden()
                        })
                    } else {
                        el.invisible().group_hover(group, |el| el.visible())
                    }
                },
            )
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
                    let Some((&row, rest)) = path.split_first() else {
                        return;
                    };
                    this.shut_menu();
                    if let Some(act) = acts.get(row) {
                        act(this, rest, window, cx);
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
