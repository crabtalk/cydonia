//! The window's strip: a tab for each entry and space the window holds, and
//! the window's own controls at its ends — past the lights, the fold while the
//! sidebar is shut, the `+`, the right panel's toggle and the right caption
//! buttons.

use crate::{
    model::workspace::{Slot, Workspace},
    view::{
        component::panel::changes_toggle,
        kit::menu,
        pane::arrangement,
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
        self, Context, Entity, MouseButton, SharedString, WeakEntity, Window, div, prelude::*, px,
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

/// Between the controls in the strip.
const BAR_GAP: f32 = 2.;

/// Which of the strip's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum StripMenu {
    /// The `+`: what to start in a tab of its own.
    Add,
    /// An entry's tab, right-pressed: its entry's own menu.
    Tab(Member),
}

/// The window's strip. Drawn `.cached(...)`: it observes the workspace, and
/// the window pushes it where it stands — see [`Strip::seat`].
pub(crate) struct Strip {
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    reorder: tabs::Reorder<Dragged>,
    menu: ContextMenu<Self, StripMenu>,
    /// Whether the pointer is over the strip: its `+` takes room there only
    /// then.
    hovered: bool,
    sidebar_open: bool,
    /// Whether the strip ends at the window's top right: the right panel is
    /// down, so the caption buttons and the panel's toggle are the strip's.
    last: bool,
}

impl Strip {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            root,
            reorder: tabs::Reorder::new(),
            menu: ContextMenu::default(),
            hovered: false,
            sidebar_open: true,
            last: true,
        }
    }

    /// Where the strip stands against the window's own chrome.
    pub(crate) fn seat(&mut self, sidebar_open: bool, last: bool, cx: &mut Context<Self>) {
        if (self.sidebar_open, self.last) != (sidebar_open, last) {
            self.sidebar_open = sidebar_open;
            self.last = last;
            cx.notify();
        }
    }

    /// Open `menu` at `place` with the rows the window builds for it, or shut
    /// it where it is the one up.
    fn toggle_menu(
        &mut self,
        menu: StripMenu,
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

    /// One tab: its name, and the `×` that takes it off the strip. An entry's
    /// tab opens its entry's menu on a right press, and reads in italic while
    /// it is the preview.
    fn tab(
        &self,
        slot: &Slot,
        front: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let workspace = self.workspace.read(cx);
        let (key, label, entry) = match slot {
            Slot::Entry(member) => {
                let toolbar = self
                    .root
                    .upgrade()
                    .and_then(|root| root.read(cx).member_toolbar(member, cx));
                // A number is a project's own, and the strip can hold entries
                // of several — so the tab says which project it is counted in.
                let project = workspace
                    .projects
                    .iter()
                    .find(|open| Some(open.path.as_path()) == member.project())
                    .map(|open| open.name())
                    .unwrap_or_default();
                let title = toolbar
                    .as_ref()
                    .map(|toolbar| toolbar.title.clone())
                    .unwrap_or_default();
                let mut label = tabs::Label::new(title);
                // A number is the badge rather than part of the name: the name
                // truncates and the reference has to survive that.
                if let Some(number) = toolbar.as_ref().and_then(|toolbar| toolbar.number) {
                    label = label.with_badge(format!("{project}#{number}"));
                }
                let entry: Option<(Row, bool)> = toolbar
                    .as_ref()
                    .and_then(|toolbar| toolbar.entry.as_ref())
                    .map(|entry| (entry.row.clone(), entry.archived));
                (arrangement::key_of(member), label, entry)
            }
            Slot::Space { space } => {
                let name = workspace
                    .space_ix(space)
                    .and_then(|ix| workspace.spaces.get(ix))
                    .map(|space| space.label().to_owned())
                    .unwrap_or_default();
                (
                    SharedString::from(format!("space::{space}")),
                    tabs::Label::new(name).with_icon(icons::layout::LayoutDashboard),
                    None,
                )
            }
        };
        let preview = slot.entry().is_some() && slot.entry() == workspace.preview_tab();
        let state = match front {
            true => tabs::State::Focused,
            false => tabs::State::Resting,
        };
        let el = tabs::tab(theme, key.clone(), label, state)
            .when(preview, |tab| tab.italic())
            .on_click(cx.listener({
                let shown = slot.clone();
                move |this, click: &gpui::ClickEvent, window, cx| {
                    // A second press keeps a preview tab.
                    if click.click_count() > 1
                        && let Some(member) = shown.entry()
                    {
                        let member = member.clone();
                        this.workspace
                            .update(cx, |workspace, cx| workspace.keep_tab(&member, cx));
                    }
                    let shown = shown.clone();
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.show_slot(&shown, window, cx)
                    });
                }
            }));
        let el = match (slot, entry) {
            (Slot::Entry(member), Some((row, archived))) => {
                let menu_key = StripMenu::Tab(member.clone());
                el.on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let (menu_key, member) = (menu_key.clone(), member.clone());
                        move |this, press: &gpui::MouseDownEvent, window, cx| {
                            let (row, host) = (row.clone(), Host::Tab(member.clone()));
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
                    "strip-tab-menu-card",
                    |this| &mut this.menu,
                    window,
                    cx,
                ))
            }
            _ => el,
        };
        el.child(
            tabs::close(theme, key, tabs::Close::OnHover)
                .tooltip(move |window, cx| Tooltip::text("Close tab", window, cx))
                .on_click(cx.listener({
                    let shut = slot.clone();
                    move |this, _, window, cx| {
                        cx.stop_propagation();
                        let shut = shut.clone();
                        this.defer_window(window, cx, move |root, window, cx| {
                            root.close_slot(&shut, window, cx)
                        });
                    }
                })),
        )
    }

    /// The `+`, revealed while the pointer is over the strip: the sidebar's
    /// `+` for `project`, landing as a tab of its own.
    fn add_button(
        &self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let open = self.menu.key() == Some(&StripMenu::Add);
        let button = menu::button(
            "strip-add",
            icons::math::Plus,
            menu::Reveal::Shown(self.hovered),
            open,
            cx,
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.toggle_menu(
                StripMenu::Add,
                Place::Below,
                move |root, _, cx| root.new_tab_rows(project, cx),
                window,
                cx,
            );
        }));
        ContextMenu::trigger(button, |this: &mut Self| &mut this.menu, StripMenu::Add, cx).children(
            self.menu.render_for(
                &StripMenu::Add,
                "strip-add-card",
                |this| &mut this.menu,
                window,
                cx,
            ),
        )
    }
}

impl InWindow for Strip {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for Strip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let strip: Vec<Slot> = workspace.strip().to_vec();
        let front = workspace.front_slot().cloned();
        let floating = workspace.settings.appearance.floating;
        // The project a `+` makes its entry in: the front entry's, else the
        // one in front of the sidebar.
        let project = front
            .as_ref()
            .and_then(Slot::entry)
            .and_then(|member| workspace.showing_of(member, cx))
            .map(|(project, _)| project)
            .or(workspace.active);
        // The lights are the window's and are drawn over whatever is at its
        // top left. Fullscreen has none.
        let fold = !self.sidebar_open;
        let left = fold && chrome::has(CaptionSide::Left, window, cx);
        let right = self.last && chrome::has(CaptionSide::Right, window, cx);
        let lead = match self.sidebar_open {
            _ if left => 0.,
            // Floating, the column pads itself by half the gap between cards
            // and the strip keeps the other half, so it starts a gap in.
            false => root::toolbar_inset(window, cx) - root::card_gap(floating),
            true if floating => 0.,
            true => root::HEADER_INSET,
        };
        let owner = cx.entity().downgrade();
        let (fold_controls, grip) = match self.root.upgrade() {
            Some(root) => root.update(cx, |root, cx| {
                (
                    fold.then(|| root.fold_controls(cx).into_any_element()),
                    chrome::grip("strip-grip", &root.drag, window),
                )
            }),
            None => (None, div().into_any_element()),
        };
        let tabs: Vec<_> = strip
            .iter()
            .map(|slot| {
                let el = self.tab(slot, Some(slot) == front.as_ref(), &theme, window, cx);
                (Dragged::Tab(slot.clone()), el)
            })
            .collect();
        let mut model = tabs::Strip::new();
        for slot in &strip {
            model.open(Dragged::Tab(slot.clone()));
        }
        if let Some(front) = &front {
            model.activate(&Dragged::Tab(front.clone()));
        }
        root::band()
            .id("window-strip")
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
                        // still inside the strip. Track its bounds instead.
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
            // edge, so with the sidebar gone it is the strip's.
            .children(fold_controls.map(|controls| {
                div()
                    .mr(px(root::HEADER_INSET - BAR_GAP))
                    .child(controls)
                    .into_any_element()
            }))
            // The tabs in a strip of their own, which scrolls sideways once
            // they no longer fit: the controls keep their places while it does.
            .child(
                self.reorder
                    .bar("window-tabs", &model, tabs)
                    .on_reorder(cx.listener(move |this, moved: &tabs::Move, _, cx| {
                        if let (Some(moving), Some(to)) =
                            (strip.get(moved.from).cloned(), strip.get(moved.to).cloned())
                        {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.reorder_slot(&moving, &to, cx)
                            });
                        }
                    })),
            )
            .children(project.map(|project| self.add_button(project, window, cx)))
            .child(grip)
            .children(
                self.last
                    .then(|| changes_toggle("Show right panel", Painter::of(cx), Theme::of(cx))),
            )
            .children(
                right
                    .then(|| chrome::caption(CaptionSide::Right, window, cx))
                    .flatten(),
            )
    }
}
