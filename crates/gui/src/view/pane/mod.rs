//! Panes: their arrangement in a space, and what every pane carries.

pub mod arrangement;
pub mod drawer;
pub mod find;
pub mod footer;
pub mod header;
pub mod items;
pub mod tab;

pub(crate) use tab::{Tab, TabMenu};

use crate::{
    model::workspace::Workspace,
    view::{
        component::panel::changes_toggle,
        kit::menu,
        sidebar::{Dragged, Row},
        window::{
            chrome,
            entry::Host,
            reach::InWindow,
            root::{self, Cydonia},
        },
    },
};
use artifact::space::Member;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, MouseButton, SharedString, StyleRefinement,
        WeakEntity, Window, div, prelude::*, px,
    },
    motion::Painter,
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        tabs,
        titlebar::CaptionSide,
        tooltip::Tooltip,
    },
};
use std::rc::Rc;

/// What the pane's name is padded by, and what a bar that is not the window's
/// leading one starts its name at: the fill that marks the focused pane needs
/// room around the text, and room off the pane's own edge so it does not run
/// into the seam.
const TAB_INSET: f32 = 8.;

/// Between the controls in a pane's bar.
const BAR_GAP: f32 = 2.;

/// Which of a pane's bar menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum BarMenu {
    /// The `···`: what can be done to the pane.
    Pane,
    /// The `+`: what to start in the pane.
    Add,
    /// A tab, right-pressed: its entry's own menu.
    Tab(Member),
}

/// Where a pane stands in the window, as its bar keeps clear of the window's
/// own chrome — pushed by [`Cydonia::sync_panes`].
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Seat {
    /// At the window's top left: the lights' clearance, and the sidebar's
    /// fold while the sidebar is shut.
    pub(crate) first: bool,
    /// At the window's top right: the caption buttons and the right panel's
    /// toggle.
    pub(crate) last: bool,
    pub(crate) sidebar_open: bool,
}

/// One pane of a space: the tabs it holds, which of them is in front, and its
/// bar.
pub(crate) struct Pane {
    /// The pane's name: the first entry of its strip, which is what the space
    /// keeps and what every drop and close is aimed at.
    pub(crate) name: Member,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// The tab of each entry of the strip, in the strip's order.
    pub(crate) tabs: Vec<Entity<Tab>>,
    /// The tabs brought to the front since the pane was made, most recent
    /// last — what [`Self::front_after`] reads. Runtime only; the tab in
    /// front is the space's.
    fronts: Vec<Member>,
    strip: tabs::Reorder<Dragged>,
    menu: ContextMenu<Self, BarMenu>,
    /// Whether the pointer is over the bar: its `+` and `···` take room there
    /// only then.
    hovered: bool,
    pub(crate) seat: Seat,
}

impl Pane {
    pub(crate) fn new(
        name: Member,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            name,
            workspace,
            root,
            tabs: Vec::new(),
            fronts: Vec::new(),
            strip: tabs::Reorder::new(),
            menu: ContextMenu::default(),
            hovered: false,
            seat: Seat::default(),
        }
    }

    /// The pane's strip, as the space orders it.
    pub(crate) fn stack(&self, cx: &App) -> Vec<Member> {
        self.workspace.read(cx).stack_of(&self.name)
    }

    /// The entry in front.
    pub(crate) fn front(&self, cx: &App) -> Member {
        self.workspace
            .read(cx)
            .front_of(&self.name)
            .unwrap_or_else(|| self.name.clone())
    }

    pub(crate) fn set_front(&mut self, front: Member, cx: &mut Context<Self>) {
        let stack = self.stack(cx);
        self.fronts
            .retain(|seen| *seen != front && stack.contains(seen));
        self.fronts.push(front.clone());
        self.workspace
            .update(cx, |workspace, cx| workspace.set_front(&front, cx));
        cx.notify();
    }

    /// The tab in front once `gone` leaves the strip: the one in front now
    /// where that is not `gone`; else the one in front before it, or else its
    /// neighbour — to its left, to its right for the first. `None` where
    /// `gone` is the strip's last tab, or not in it.
    pub(crate) fn front_after(&self, gone: &Member, cx: &App) -> Option<Member> {
        let stack = self.stack(cx);
        let at = stack.iter().position(|tab| tab == gone)?;
        let front = self.front(cx);
        if front != *gone {
            return Some(front);
        }
        self.fronts
            .iter()
            .rev()
            .find(|tab| *tab != gone && stack.contains(tab))
            .or_else(|| match at {
                0 => stack.get(1),
                at => stack.get(at - 1),
            })
            .cloned()
    }

    /// The tab in front.
    pub(crate) fn front_tab(&self, cx: &App) -> Option<Entity<Tab>> {
        let front = self.front(cx);
        self.tabs
            .iter()
            .find(|tab| tab.read(cx).entry.as_ref() == Some(&front))
            .cloned()
    }

    /// Open `menu` at `place` with the rows the window builds for it, or shut
    /// it where it is the one up.
    fn toggle_menu(
        &mut self,
        menu: BarMenu,
        place: Place,
        rows: impl FnOnce(
            &mut Cydonia,
            &mut Window,
            &mut Context<Cydonia>,
        ) -> Vec<(Item, menu::Act<Cydonia>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rows) = self
            .root
            .update(cx, |root, cx| Self::through(rows(root, window, cx)))
            .ok()
        else {
            return;
        };
        let rows = std::cell::RefCell::new(Some(rows));
        ContextMenu::toggle(
            self,
            |this| &mut this.menu,
            menu,
            place,
            move |_, _, _| menu::chooser(rows.borrow_mut().take().unwrap_or_default()),
            window,
            cx,
        );
    }

    /// One tab: what it is on, and the `×` that takes it out. A right press
    /// opens its entry's menu.
    ///
    /// Every tab carries the close, the pane's first included. There is no
    /// separate control for closing the pane, because there is no separate
    /// thing to close: a pane is its strip, and the last `×` takes the pane
    /// with the tab. The focus mark is on the tab rather than on the pane,
    /// since the panes are one plane divided and take no fill of their own.
    fn tab(
        &self,
        tab: &Member,
        front: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let toolbar = self
            .root
            .upgrade()
            .and_then(|root| root.read(cx).member_toolbar(tab, cx));
        // A number is a project's own, and an arrangement can hold panes from
        // several — so the tab says which project the number is counted in.
        let project = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .find(|open| Some(open.path.as_path()) == tab.project())
            .map(|open| open.name())
            .unwrap_or_default();
        // The pane in front *and* the tab in front of that pane: a background
        // pane's own front tab is not where the window's attention is.
        let focused = front
            && self
                .tabs
                .iter()
                .any(|held| held.read(cx).entry.as_ref() == Some(tab) && held.read(cx).focused);
        let key = arrangement::key_of(tab);
        let title = SharedString::from(
            toolbar
                .as_ref()
                .map(|toolbar| toolbar.title.clone())
                .unwrap_or_default(),
        );
        let mut label = tabs::Label::new(title.clone());
        // A number is the badge rather than part of the name: the name
        // truncates and the reference has to survive that.
        if let Some(number) = toolbar.as_ref().and_then(|toolbar| toolbar.number) {
            label = label.with_badge(format!("{project}#{number}"));
        }
        // A tab that is showing but not focused still has to read as the one
        // its pane is on, or a background pane's strip says nothing about what
        // is under it — which is [`tabs::State::Front`].
        let state = match (focused, front) {
            (true, _) => tabs::State::Focused,
            (false, true) => tabs::State::Front,
            (false, false) => tabs::State::Resting,
        };
        let entry: Option<(Row, bool)> = toolbar
            .as_ref()
            .and_then(|toolbar| toolbar.entry.as_ref())
            .map(|entry| (entry.row.clone(), entry.archived));
        let menu_key = BarMenu::Tab(tab.clone());
        tabs::tab(theme, key.clone(), label, state)
            .on_click(cx.listener({
                let (on, shown) = (self.name.clone(), tab.clone());
                move |this, _, window, cx| {
                    let (on, shown) = (on.clone(), shown.clone());
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.show_tab(&on, &shown, window, cx)
                    });
                }
            }))
            // The entry's own menu, the one its band's `···` opens, where the
            // press lands.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let (menu_key, shown_tab) = (menu_key.clone(), tab.clone());
                    move |this, press: &gpui::MouseDownEvent, window, cx| {
                        let Some((row, archived)) = entry.clone() else {
                            return;
                        };
                        let host = Host::Tab(shown_tab.clone());
                        this.toggle_menu(
                            menu_key.clone(),
                            Place::At(press.position),
                            move |root, _, cx| root.entry_rows(host, &row, archived, cx),
                            window,
                            cx,
                        );
                    }
                }),
            )
            .children(self.menu.render_for(
                &menu_key,
                "pane-tab-menu-card",
                |this| &mut this.menu,
                window,
                cx,
            ))
            .child(
                tabs::close(theme, key, tabs::Close::OnHover)
                    .tooltip(move |window, cx| Tooltip::text("Close tab", window, cx))
                    .on_click(cx.listener({
                        let shut = tab.clone();
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            let shut = shut.clone();
                            this.defer_window(window, cx, move |root, window, cx| {
                                root.close_pane(&shut, window, cx)
                            });
                        }
                    })),
            )
    }

    /// A `···` or `+` on the bar that opens `menu` below itself, revealed
    /// while the pointer is over the bar.
    fn bar_button(
        &self,
        id: SharedString,
        mark: &'static [u8],
        menu: BarMenu,
        rows: impl Fn(&mut Cydonia, &App) -> Vec<(Item, menu::Act<Cydonia>)> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (open, rows) = (self.menu.key() == Some(&menu), Rc::new(rows));
        let button = menu::button(
            id.clone(),
            mark,
            menu::Reveal::Shown(self.hovered),
            open,
            cx,
        )
        .on_click(cx.listener({
            let menu = menu.clone();
            move |this, _, window, cx| {
                cx.stop_propagation();
                let rows = rows.clone();
                this.toggle_menu(
                    menu.clone(),
                    Place::Below,
                    move |root, _, cx| rows(root, cx),
                    window,
                    cx,
                );
            }
        }));
        ContextMenu::trigger(button, |this: &mut Self| &mut this.menu, menu.clone(), cx).children(
            self.menu.render_for(
                &menu,
                SharedString::from(format!("{id}-card")),
                |this| &mut this.menu,
                window,
                cx,
            ),
        )
    }

    /// The pane's bar: the strip of its tabs, and the pane's own controls.
    fn bar(&self, theme: &Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Seat {
            first,
            last,
            sidebar_open,
        } = self.seat;
        let key = arrangement::key_of(&self.name);
        let stack = self.stack(cx);
        let front = self.front(cx);
        // The lights are the window's and are drawn over whatever is at its
        // top left, so their clearance is taken by the pane that lands there
        // and nowhere another pane can see it. Fullscreen has none.
        let fold = first && !sidebar_open;
        let left = fold && chrome::has(CaptionSide::Left, window, cx);
        let right = last && chrome::has(CaptionSide::Right, window, cx);
        let lead = match (first, sidebar_open) {
            _ if left => 0.,
            (true, true) => root::HEADER_INSET,
            (true, false) => root::toolbar_inset(
                root::content_edge(self.workspace.read(cx).settings.appearance.floating),
                window,
                cx,
            ),
            (false, _) => TAB_INSET,
        };
        let owner = cx.entity().downgrade();
        let project = self
            .workspace
            .read(cx)
            .showing_of(&front, cx)
            .map(|(project, _)| project);
        let (fold_controls, grip) = match self.root.upgrade() {
            Some(root) => root.update(cx, |root, cx| {
                (
                    fold.then(|| root.fold_controls(cx).into_any_element()),
                    chrome::grip(
                        SharedString::from(format!("pane-grip-{key}")),
                        &root.drag,
                        window,
                    ),
                )
            }),
            None => (None, div().into_any_element()),
        };
        root::band()
            .id(SharedString::from(format!("pane-bar-{key}")))
            .group("pane-bar")
            .relative()
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let exited = owner.clone();
                        window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, _, cx| {
                            if phase != gpui::DispatchPhase::Capture {
                                return;
                            }
                            let _ = exited.update(cx, |this, cx| {
                                if this.hovered {
                                    this.hovered = false;
                                    cx.notify();
                                }
                            });
                        });
                        let owner = owner.clone();
                        // The scrollbar blocks hitbox hover beneath it, but is
                        // still inside the bar. Track the bar's bounds instead.
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase != gpui::DispatchPhase::Capture {
                                return;
                            }
                            let _ = owner.update(cx, |this, cx| {
                                let over = bounds.contains(&event.position);
                                if this.hovered != over {
                                    this.hovered = over;
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .w_full()
            .gap(px(BAR_GAP))
            .pl(px(lead))
            .when(right, |el| el.pr_0())
            .children(
                left.then(|| chrome::caption(CaptionSide::Left, window, cx))
                    .flatten(),
            )
            // The fold belongs to whichever column runs along the window's left
            // edge, so with the sidebar gone it is this pane's. The tabs keep
            // the inset off it that they keep off the sidebar's edge.
            .children(fold_controls.map(|controls| {
                div()
                    .mr(px(root::HEADER_INSET - BAR_GAP))
                    .child(controls)
                    .into_any_element()
            }))
            // The tabs in a strip of their own, which scrolls sideways once
            // they no longer fit: the bar's other children are the pane's
            // chrome and keep their places while it does.
            .child({
                let mut strip = tabs::Strip::new();
                for tab in &stack {
                    strip.open(Dragged::Tab(tab.clone()));
                }
                strip.activate(&Dragged::Tab(front.clone()));
                let tabs: Vec<_> = stack
                    .iter()
                    .map(|tab| {
                        let el = self.tab(tab, *tab == front, theme, window, cx);
                        (Dragged::Tab(tab.clone()), el)
                    })
                    .collect();
                self.strip
                    .bar(
                        SharedString::from(format!("pane-strip-{key}")),
                        &strip,
                        tabs,
                    )
                    .on_reorder(cx.listener({
                        let stack = stack.clone();
                        move |this, moved: &tabs::Move, window, cx| {
                            if let (Some(moving), Some(to)) =
                                (stack.get(moved.from).cloned(), stack.get(moved.to).cloned())
                            {
                                this.defer_window(window, cx, move |root, _, cx| {
                                    root.reorder_tab(&moving, &to, cx);
                                });
                            }
                        }
                    }))
            })
            .children(project.map(|project| {
                let name = self.name.clone();
                self.bar_button(
                    SharedString::from(format!("pane-add-{key}")),
                    icons::math::Plus,
                    BarMenu::Add,
                    move |root, cx| root.pane_add_rows(&name, project, cx),
                    window,
                    cx,
                )
            }))
            .child(grip)
            // The article in front's comments, beside the pane's `···`.
            .children(
                self.front_tab(cx)
                    .and_then(|tab| tab.read(cx).article_shown(cx))
                    .map(|view| view.read(cx).comments.clone()),
            )
            .child({
                let name = self.name.clone();
                self.bar_button(
                    SharedString::from(format!("pane-menu-{key}")),
                    icons::layout::Ellipsis,
                    BarMenu::Pane,
                    move |root, cx| root.pane_rows(&name, cx),
                    window,
                    cx,
                )
            })
            // The band's right-panel button, on the pane at the window's top
            // right — which is `last` only while the panel is down.
            .children(
                last.then(|| changes_toggle("Show right panel", Painter::of(cx), Theme::of(cx))),
            )
            .children(
                right
                    .then(|| chrome::caption(CaptionSide::Right, window, cx))
                    .flatten(),
            )
            .into_any_element()
    }
}

impl InWindow for Pane {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let key = arrangement::key_of(&self.name);
        let front = self.front(cx);
        let tab = self.front_tab(cx);
        let focus = tab.as_ref().map(|tab| tab.read(cx).focus.clone());
        let body = match tab {
            Some(tab) => tab
                .cached(StyleRefinement::default().flex().flex_col().size_full())
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };
        let root = self.root.clone();
        div()
            .id(SharedString::from(format!("pane-{key}")))
            // With the context but without this, a pane claims chords that
            // never reach it: an action runs through the focused element's
            // ancestors, and a pane showing a board holds nothing that takes
            // the focus — see [`Tab::focus`]. The front tab's handle: the
            // body, the composer and [`Cydonia::focus_pane`] all answer for
            // the tab in front.
            .when_some(focus, |pane, focus| pane.track_focus(&focus))
            .group("pane")
            .size_full()
            .min_w_0()
            .min_h_0()
            // A flex column, not just a box: every pane draws itself with
            // `flex_1`, which fills nothing at all outside one.
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            // No fill: the panes sit on the detail column's one surface, and a
            // wash per pane would draw the arrangement as a row of cards
            // rather than one plane divided. The focus is said on the tab.
            //
            // Pressing anywhere in a pane is how the focus moves to it, the
            // same way a click into the sidebar selects a row.
            .on_mouse_down(MouseButton::Left, {
                let (root, on) = (root.clone(), front.clone());
                move |_, window, cx| {
                    root.update(cx, |root, cx| root.focus_pane(&on, window, cx))
                        .ok();
                }
            })
            // Before anything inside takes the press: a link pressed in this
            // pane opens in its drawer.
            .capture_any_mouse_down({
                let on = front.clone();
                move |_, _, cx| {
                    root.update(cx, |root, _| root.pressed_pane = Some(on.clone()))
                        .ok();
                }
            })
            .child(self.bar(&theme, window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(body),
            )
    }
}
