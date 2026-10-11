//! The `···` and `+` menus every sidebar row, pane and column heading opens,
//! on the window's one [`ContextMenu`], keyed by [`Menu`].

use crate::view::{sidebar::Row, window::root::Cydonia};
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
pub(crate) type Act = Box<dyn Fn(&mut Cydonia, &[usize], &mut Window, &mut Context<Cydonia>)>;

/// The rows a menu opens with, built when it opens.
pub(crate) type Rows =
    Box<dyn Fn(&mut Cydonia, &mut Window, &mut Context<Cydonia>) -> Vec<(Item, Act)>>;

/// Which menu is open.
///
/// Cloned rather than copied since [`Menu::Card`] names its card, which is a
/// string — a card is addressed by id everywhere else and a menu key that used
/// its position would open the wrong one the moment a card moved.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Menu {
    /// The `+` on a project heading, by the project's path: what to start
    /// here.
    Add(std::path::PathBuf),
    /// A project heading, by the project's path: what to do to the project.
    Project(std::path::PathBuf),
    /// The `···` on an entry's row, whichever kind it is.
    Entry(Row),
    /// The `···` on an entry's row in the library. Its own key: the same entry
    /// has a sidebar row, and one key would open both menus.
    Library(Row),
    /// The library's tabs, as a select in a narrow bar.
    LibraryTab,
    /// The library's project picker.
    LibraryProject,
    /// The library selection's `Move to`.
    LibraryMove,
    /// The library's Status heading.
    LibraryStatus,
    /// The library's Agent heading.
    LibraryAgent,
    /// The `···` in the pane header. Its own key rather than `Entry` of what
    /// the header is showing: that entry has a row in the sidebar too, and a
    /// key naming the entry would have one click open both of them.
    Header,
    /// The `···` in one pane's bar, by the entry the pane is on. A space has
    /// several bars on screen at once, so `Header` alone would open every one
    /// of them together.
    Pane(gpui::SharedString),
    /// A pane's tab, right-pressed, by the entry the tab is on.
    Tab(artifact::space::Member),
    /// The `+` in one pane's bar, by the same key as [`Menu::Pane`].
    PaneAdd(gpui::SharedString),
    /// The `···` on a table's column heading.
    Column(usize),
    /// The `···` on a board's lane, by column id. An id and not a position: a
    /// lane's whole menu is about moving it, and a key that moved with it would
    /// shut the menu on every press.
    Lane(String),
    /// The agents on offer under `New session`, on the screen a project with
    /// nothing open shows — see [`crate::view::window::root::Cydonia::launch`].
    Launch,
    /// The block picker on the article ribbon — see
    /// [`crate::view::component::ribbon`].
    Turn,
    /// The `···` on a card, by card id.
    Card(String),
    /// An article's outline, by the article's file — a space can show several.
    Outline(std::path::PathBuf),
    /// An article's comments, by the article's file.
    Comments(std::path::PathBuf),
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

/// Groups of rows, one after another with a separator between each two. Empty
/// groups are dropped, so no separator stands at an end or beside another.
pub(crate) fn sections(groups: impl IntoIterator<Item = Vec<(Item, Act)>>) -> Vec<(Item, Act)> {
    let mut rows = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !rows.is_empty() {
            // A hit names a row by its position, so the separator holds one
            // too, with nothing behind it.
            let nothing: Act = Box::new(|_, _, _, _| {});
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
        bezel::ui::popover::close_popup(self, cx, |this: &mut Self| &mut this.picker);
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
        let id = id.into();
        let open = self.menu_open(&menu);
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
            .when_some(group.filter(|_| !open), |el, group| {
                // Resolve space during render, never in a hover style:
                // GPUI can resolve hover differently in prepaint and paint.
                match &menu {
                    Menu::Entry(_) => el.when(self.sidebar_hovered.as_ref() != Some(&menu), |el| {
                        el.hidden()
                    }),
                    // The project's head reports its hover as its `···`.
                    Menu::Add(ix) => el.when(
                        !matches!(&self.sidebar_hovered, Some(Menu::Project(at)) if at == ix),
                        |el| el.hidden(),
                    ),
                    Menu::Pane(pane) | Menu::PaneAdd(pane) => {
                        el.when(self.pane_hovered.as_ref() != Some(pane), |el| el.hidden())
                    }
                    _ => el.invisible().group_hover(group, |el| el.visible()),
                }
            })
            .on_click({
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
fn chooser(rows: Vec<(Item, Act)>) -> (Vec<Item>, Act) {
    let (items, acts): (Vec<Item>, Vec<Act>) = rows.into_iter().unzip();
    let choose: Act = Box::new(move |this, path, window, cx| {
        if let Some((&row, rest)) = path.split_first()
            && let Some(act) = acts.get(row)
        {
            act(this, rest, window, cx);
        }
        cx.notify();
    });
    (items, choose)
}
