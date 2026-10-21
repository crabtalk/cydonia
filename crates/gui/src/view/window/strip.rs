//! The window's strip: a tab for each entry and space the window holds, and
//! the window's own controls at its ends — past the lights, the fold while the
//! sidebar is shut, the right panel's toggle and the right caption buttons.

use crate::{
    model::workspace::{Slot, Workspace},
    view::{
        component::panel::changes_toggle,
        kit::{
            icon, menu,
            rename::{NameEvent, NameField},
        },
        pane::arrangement,
        sidebar::{Dragged, Renaming},
        window::{
            chrome,
            entry::Host,
            reach::InWindow,
            root::{self, Cydonia},
        },
    },
};
use bezel::{
    gpui::{
        self, Context, Entity, MouseButton, SharedString, WeakEntity, Window, div, prelude::*, px,
    },
    motion::Painter,
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::Case,
        menu::Item,
        tabs,
        titlebar::CaptionSide,
    },
};

/// Between the controls in the strip.
const BAR_GAP: f32 = 2.;
/// The empty row a track of tabs stops short of the toggle by.
const DRAG_ROOM: f32 = 32.;

/// Which of the strip's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum StripMenu {
    /// A tab, right-pressed: its entry's own menu, or its space's.
    Tab(Slot),
}

/// The window's strip. Drawn `.cached(...)`: it observes the workspace, and
/// the window pushes it where it stands — see [`Strip::seat`].
pub(crate) struct Strip {
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    reorder: tabs::Reorder<Dragged>,
    menu: ContextMenu<Self, StripMenu>,
    /// The tab holding the name field, and what it renames.
    renaming: Option<(Slot, Renaming)>,
    name_field: Entity<NameField>,
    sidebar_open: bool,
    /// Whether the right panel is up, which the toggle at the strip's end
    /// says.
    panel_open: bool,
}

impl Strip {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        let name_field = cx.new(NameField::new);
        cx.subscribe(&name_field, |this, _, event: &NameEvent, cx| {
            let Some((_, what)) = this.renaming.take() else {
                return;
            };
            if let NameEvent::Named(name) = event {
                let name = name.clone();
                this.workspace
                    .update(cx, |workspace, cx| what.commit(name, workspace, cx));
            }
            cx.notify();
        })
        .detach();
        Self {
            workspace,
            root,
            reorder: tabs::Reorder::new(),
            menu: ContextMenu::default(),
            renaming: None,
            name_field,
            sidebar_open: true,
            panel_open: false,
        }
    }

    /// Where the strip stands against the window's own chrome.
    pub(crate) fn seat(&mut self, sidebar_open: bool, panel_open: bool, cx: &mut Context<Self>) {
        if (self.sidebar_open, self.panel_open) != (sidebar_open, panel_open) {
            self.sidebar_open = sidebar_open;
            self.panel_open = panel_open;
            cx.notify();
        }
    }

    /// Put the name field on `slot`'s tab, renaming `what`.
    pub(crate) fn start_rename(
        &mut self,
        slot: Slot,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = what.label(self.workspace.read(cx), cx);
        self.name_field
            .update(cx, |field, cx| field.start(label, Case::Mixed, window, cx));
        self.renaming = Some((slot, what));
        cx.notify();
    }

    /// The rows of `slot`'s menu: its entry's own, or the row renaming its
    /// space.
    fn slot_rows(
        slot: Slot,
    ) -> impl FnOnce(&mut Cydonia, &mut Window, &mut Context<Cydonia>) -> Vec<(Item, menu::Act<Cydonia>)>
    {
        move |root, _, cx| match slot {
            Slot::Entry(member) => root
                .member_toolbar(&member, cx)
                .and_then(|toolbar| toolbar.entry)
                .map(|entry| {
                    root.entry_rows(Host::Strip(member.clone()), &entry.row, entry.archived, cx)
                })
                .unwrap_or_default(),
            Slot::Space { space } => {
                let rename = menu::row::<Cydonia>(
                    Item::action("Rename").with_icon(icons::text::SquarePen),
                    {
                        let space = space.clone();
                        move |root, window, cx| {
                            let (slot, what) = (
                                Slot::Space {
                                    space: space.clone(),
                                },
                                Renaming::Space(space.clone()),
                            );
                            root.strip
                                .update(cx, |strip, cx| strip.start_rename(slot, what, window, cx));
                        }
                    },
                );
                let close = menu::row::<Cydonia>(
                    Item::action("Close tab").with_icon(icons::notifications::X),
                    move |root, window, cx| {
                        let slot = Slot::Space {
                            space: space.clone(),
                        };
                        root.close_slot(&slot, window, cx)
                    },
                );
                menu::sections([vec![rename], vec![close]])
            }
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

    /// One tab: its kind's mark and its name. A right press opens its menu,
    /// which closes it: an entry's own menu, or a space's, whose closing
    /// deletes the space. An entry's reads in italic while it is the preview.
    fn tab(
        &self,
        slot: &Slot,
        front: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let workspace = self.workspace.read(cx);
        let renaming = self.renaming.as_ref().is_some_and(|(at, _)| at == slot);
        let (key, label, menu) = match slot {
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
                    .filter(|_| !renaming)
                    .map(|toolbar| toolbar.title.clone())
                    .unwrap_or_default();
                let mut label =
                    tabs::Label::new(title).with_icon(icon::entry_icon(member, workspace, cx));
                // A number is the badge rather than part of the name: the name
                // truncates and the reference has to survive that.
                if let Some(number) = toolbar
                    .as_ref()
                    .and_then(|toolbar| toolbar.number)
                    .filter(|_| !renaming)
                {
                    label = label.with_badge(format!("{project}#{number}"));
                }
                let menu = toolbar.is_some_and(|toolbar| toolbar.entry.is_some());
                (arrangement::key_of(member), label, menu)
            }
            Slot::Space { space } => {
                let name = match renaming {
                    true => String::new(),
                    false => workspace
                        .space_ix(space)
                        .and_then(|ix| workspace.spaces.get(ix))
                        .map(|space| space.label().to_owned())
                        .unwrap_or_default(),
                };
                (
                    SharedString::from(format!("space::{space}")),
                    tabs::Label::new(name).with_icon(icons::layout::LayoutDashboard),
                    true,
                )
            }
        };
        let preview = slot.entry().is_some() && slot.entry() == workspace.preview_tab();
        let state = match front {
            true => tabs::State::Focused,
            false => tabs::State::Resting,
        };
        let el = tabs::tab(theme, key.clone(), label, state)
            .occlude()
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
        let el = match menu {
            true => {
                let menu_key = StripMenu::Tab(slot.clone());
                el.on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let (menu_key, slot) = (menu_key.clone(), slot.clone());
                        move |this, press: &gpui::MouseDownEvent, window, cx| {
                            this.toggle_menu(
                                menu_key.clone(),
                                Place::At(press.position),
                                Self::slot_rows(slot.clone()),
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
            false => el,
        };
        el.children(renaming.then(|| self.name_field.clone()))
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
        // The lights are the window's and are drawn over whatever is at its
        // top left. Fullscreen has none.
        let fold = !self.sidebar_open;
        let left = fold && chrome::has(CaptionSide::Left, window, cx);
        let right = chrome::has(CaptionSide::Right, window, cx);
        // The row runs edge to edge; its tabs start where the cards below do.
        let lead = match self.sidebar_open {
            _ if left => 0.,
            false => root::toolbar_inset(window, cx),
            true if floating => root::card_gap(floating),
            true => root::HEADER_INSET,
        };
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
        let tracked = strip.len() > 1 || self.renaming.is_some();
        let mut model = tabs::Strip::new();
        for slot in &strip {
            model.open(Dragged::Tab(slot.clone()));
        }
        if let Some(front) = &front {
            model.activate(&Dragged::Tab(front.clone()));
        }
        // A control over the drag layer takes its own presses.
        let shield = |control: gpui::AnyElement| div().flex_none().occlude().child(control);
        root::band()
            .id("window-strip")
            .relative()
            .w_full()
            .gap(px(BAR_GAP))
            .pl(px(lead))
            .when(right, |el| el.pr_0())
            // Under the whole row: any of it no control stands on moves the
            // window, as a titlebar does.
            .child(div().absolute().inset_0().flex().child(grip))
            .children(
                left.then(|| chrome::caption(CaptionSide::Left, window, cx))
                    .flatten()
                    .map(|caption| shield(caption.into_any_element())),
            )
            // The fold belongs to whichever column runs along the window's left
            // edge, so with the sidebar gone it is the strip's.
            .children(fold_controls.map(|controls| {
                shield(controls)
                    .mr(px(root::HEADER_INSET - BAR_GAP))
                    .into_any_element()
            }))
            // The tabs on a track of their own, which scrolls sideways once
            // they no longer fit: the controls keep their places while it does.
            // One tab has no track, back while its name is typed.
            .when(tracked, |el| {
                el.child(
                    self.reorder
                        .bar("window-tabs", &model, tabs)
                        .on_track(match floating {
                            true => tabs::Track::Bare,
                            false => tabs::Track::Bordered,
                        })
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
            })
            // Empty row kept before the toggle, so the window can be taken
            // by it however many tabs there are.
            .child(match tracked {
                true => div().flex_none().w(px(DRAG_ROOM)),
                false => div().flex_1(),
            })
            .child(shield(
                changes_toggle(
                    match self.panel_open {
                        true => "Hide right panel",
                        false => "Show right panel",
                    },
                    Painter::of(cx),
                    Theme::of(cx),
                )
                .into_any_element(),
            ))
            .children(
                right
                    .then(|| chrome::caption(CaptionSide::Right, window, cx))
                    .flatten()
                    .map(|caption| shield(caption.into_any_element())),
            )
    }
}
