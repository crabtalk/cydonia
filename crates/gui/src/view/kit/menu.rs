//! The rows and triggers every view's [`ContextMenu`] is built from, and the
//! window's own menus — the band's and the launch screen's — keyed by
//! [`Menu`].

use crate::view::window::root::Cydonia;
use bezel::{
    gpui::{self, AnyElement, Context, Div, SharedString, Stateful, Window, prelude::*},
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons::Icon,
        menu::Item,
        widgets::{ButtonStyle, Buttons},
    },
};

/// What a row does when it is picked, handed the rest of the path — which of a
/// submenu's rows it was, and empty for a row that opens nothing.
pub(crate) type Act<V = Cydonia> = Box<dyn Fn(&mut V, &[usize], &mut Window, &mut Context<V>)>;

/// The rows a menu opens with, built when it opens.
pub(crate) type Rows<V = Cydonia> =
    Box<dyn Fn(&mut V, &mut Window, &mut Context<V>) -> Vec<(Item, Act<V>)>>;

/// How a menu's trigger is revealed.
pub(crate) enum Reveal {
    /// Always on show.
    Always,
    /// While the hover group of this name is hovered.
    Group(&'static str),
    /// On show or not, as the caller decided.
    Shown(bool),
}

/// Which of the window's own menus is open.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The `···` in the band over the window's one pane.
    Header,
    /// The agents on offer under `New session`, on the screen a project with
    /// nothing open shows — see [`crate::view::window::root::Cydonia::launch`].
    Launch,
}

/// One row of a menu, and what picking it does.
pub(crate) fn row<V: 'static>(
    item: Item,
    act: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> (Item, Act<V>) {
    (
        item,
        Box::new(move |this, _, window, cx| act(this, window, cx)),
    )
}

/// A row that drops a panel of rows of its own. Picking one of those is what
/// acts; the submenu row itself only opens.
pub(crate) fn submenu<V: 'static>(
    label: impl Into<SharedString>,
    icon: impl Into<Icon>,
    rows: Vec<(Item, Act<V>)>,
) -> (Item, Act<V>) {
    let (items, acts): (Vec<Item>, Vec<Act<V>>) = rows.into_iter().unzip();
    let item = Item::submenu(label, items).with_icon(icon);
    let act: Act<V> = Box::new(move |this, path, window, cx| {
        if let Some((&at, rest)) = path.split_first()
            && let Some(act) = acts.get(at)
        {
            act(this, rest, window, cx);
        }
    });
    (item, act)
}

/// A switchable row's name with the value it is on. A submenu row is one line
/// and has no column for a value, so what a thing is set to is written into
/// what it is called.
pub(crate) fn set_to(name: &str, value: Option<SharedString>) -> SharedString {
    match value {
        Some(value) => format!("{name}: {value}").into(),
        None => name.into(),
    }
}

/// A switch between `options`: a submenu named for the one it is on, `current`,
/// with a checked row per option. Picking one hands its value to `pick`.
pub(crate) fn switch<V: 'static, T: Copy + PartialEq + 'static>(
    name: &str,
    icon: impl Into<Icon>,
    options: Vec<(T, Icon, &'static str)>,
    current: T,
    pick: impl Fn(&mut V, T, &mut Window, &mut Context<V>) + 'static,
) -> (Item, Act<V>) {
    let pick = std::rc::Rc::new(pick);
    let on = options
        .iter()
        .find(|(value, ..)| *value == current)
        .map(|(_, _, label)| SharedString::from(*label));
    let rows = options
        .into_iter()
        .map(|(value, glyph, label)| {
            let pick = pick.clone();
            row::<V>(
                Item::action(label)
                    .with_icon(glyph)
                    .checked(value == current),
                move |this, window, cx| pick(this, value, window, cx),
            )
        })
        .collect();
    submenu(set_to(name, on), icon, rows)
}

/// Groups of rows, one after another with a separator between each two. Empty
/// groups are dropped, so no separator stands at an end or beside another.
pub(crate) fn sections<V: 'static>(
    groups: impl IntoIterator<Item = Vec<(Item, Act<V>)>>,
) -> Vec<(Item, Act<V>)> {
    let mut rows = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !rows.is_empty() {
            // A hit names a row by its position, so the separator holds one
            // too, with nothing behind it.
            let nothing: Act<V> = Box::new(|_, _, _, _| {});
            rows.push((Item::Separator, nothing));
        }
        rows.extend(group);
    }
    rows
}

impl Cydonia {
    /// Open `menu` at `place` with the rows `rows` builds, or shut it when it
    /// is the one up.
    pub(crate) fn toggle_menu(
        &mut self,
        menu: Menu,
        place: Place,
        rows: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) -> Vec<(Item, Act)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        ContextMenu::toggle(
            self,
            |this| &mut this.menu,
            menu,
            place,
            |this, window, cx| chooser(rows(this, window, cx)),
            window,
            cx,
        );
    }

    /// Shut whichever menu is open, leaving the focus where it is.
    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        ContextMenu::close(self, |this| &mut this.menu, cx);
    }

    /// `el`, noting each press for `menu` so a press on the trigger of the
    /// menu that is up shuts it rather than reopening it.
    pub(crate) fn menu_trigger<E: gpui::StatefulInteractiveElement>(
        &self,
        el: E,
        menu: Menu,
        cx: &Context<Self>,
    ) -> E {
        ContextMenu::trigger(el, |this: &mut Self| &mut this.menu, menu, cx)
    }

    /// Whether `menu` is the one up.
    pub(crate) fn menu_open(&self, menu: &Menu) -> bool {
        self.menu.key() == Some(menu)
    }

    /// A `···` or `+` that opens `menu` below itself, revealed on the row's
    /// hover. `group` is the hover group that reveals it; `None` for a trigger
    /// that stands on its own, as the pane header's does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn menu_button(
        &self,
        id: impl Into<SharedString>,
        group: Option<&'static str>,
        mark: impl Into<Icon>,
        menu: Menu,
        place: Place,
        rows: Rows,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        // Resolved during render, never in a hover style: GPUI can resolve
        // hover differently in prepaint and paint.
        let reveal = match group {
            None => Reveal::Always,
            Some(group) => Reveal::Group(group),
        };
        let button = button(id, mark, reveal, self.menu_open(&menu), cx).on_click({
            let menu = menu.clone();
            let rows: std::rc::Rc<Rows> = std::rc::Rc::new(rows);
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                let rows = rows.clone();
                this.toggle_menu(
                    menu.clone(),
                    place,
                    move |this, window, cx| rows(this, window, cx),
                    window,
                    cx,
                );
            })
        });
        self.menu_trigger(button, menu, cx)
    }

    /// The menu `menu`, placed, while it is on screen: what its trigger hangs
    /// beside itself.
    pub(crate) fn menu_here(
        &self,
        menu: &Menu,
        id: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.menu
            .render_for(menu, id, |this| &mut this.menu, window, cx)
    }
}

/// `rows` as a [`ContextMenu`] takes them: the items, and what choosing the
/// one at a path runs.
pub(crate) fn chooser<V: 'static>(rows: Vec<(Item, Act<V>)>) -> (Vec<Item>, Act<V>) {
    let (items, acts): (Vec<Item>, Vec<Act<V>>) = rows.into_iter().unzip();
    let choose: Act<V> = Box::new(move |this, path, window, cx| {
        if let Some((&row, rest)) = path.split_first()
            && let Some(act) = acts.get(row)
        {
            act(this, rest, window, cx);
        }
        cx.notify();
    });
    (items, choose)
}

/// A `···` or `+` that opens a menu, as `reveal` says. `open` keeps it on
/// show whatever `reveal` says: by then the pointer is over the menu, not the
/// row that opened it.
pub(crate) fn button<V: 'static>(
    id: impl Into<SharedString>,
    mark: impl Into<Icon>,
    reveal: Reveal,
    open: bool,
    cx: &Context<V>,
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
        .relative();
    match reveal {
        _ if open => button,
        Reveal::Always => button,
        Reveal::Group(group) => button.invisible().group_hover(group, |el| el.visible()),
        Reveal::Shown(shown) => button.when(!shown, |el| el.hidden()),
    }
}
