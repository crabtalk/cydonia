//! The window's strip: a tab for each entry and space the window holds, the
//! room at its ends that the window's corner controls stand in — see
//! [`crate::view::window::chrome`].

use crate::{
    model::workspace::{Slot, Workspace},
    view::{
        kit::{
            icon,
            menu::{self, MenuHost},
            rename::{Rename, Renaming},
        },
        pane::arrangement,
        sidebar::Dragged,
        window::{
            chrome,
            entry::Host,
            reach::InWindow,
            root::{self, Cydonia},
        },
    },
};
use artifact::reference::Reference;
use bezel::{
    gpui::{
        self, App, Context, Entity, MouseButton, SharedString, WeakEntity, Window, div, prelude::*,
        px,
    },
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        tabs,
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
    /// The name field, on the tab of the space it renames.
    rename: Rename,
    sidebar_open: bool,
}

impl Strip {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        let rename = Rename::new(workspace.clone(), |this: &mut Self| &mut this.rename, cx);
        Self {
            workspace,
            root,
            reorder: tabs::Reorder::new(),
            menu: ContextMenu::default(),
            rename,
            sidebar_open: true,
        }
    }

    /// Whether the strip shows its tabs: more than one, or one being renamed.
    /// None while the sidebar lists the spaces: it says what is open.
    pub(crate) fn tracked(&self, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        workspace.settings.appearance.global_tabs()
            && (workspace.strip().len() > 1 || self.rename.what.is_some())
    }

    /// Whether the sidebar stands at the window's top left beside the strip.
    pub(crate) fn seat(&mut self, sidebar_open: bool, cx: &mut Context<Self>) {
        if self.sidebar_open != sidebar_open {
            self.sidebar_open = sidebar_open;
            cx.notify();
        }
    }

    /// Put the name field on the tab of the space `id`.
    fn rename_space(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.rename.start(Renaming::Space(id), window, cx);
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
                    root.entry_rows(Host::Tab(member.clone()), &entry.row, entry.archived, cx)
                })
                .unwrap_or_default(),
            Slot::Space { space } => vec![menu::row::<Cydonia>(
                Item::action("Rename").with_icon(icons::text::SquarePen),
                move |root, window, cx| {
                    let id = space.clone();
                    root.strip
                        .update(cx, |strip, cx| strip.rename_space(id, window, cx));
                },
            )],
        }
    }

    /// One tab: its kind's mark and its name. A right press opens its menu: an
    /// entry's own menu, or a space's. An entry's reads in italic while it is
    /// the preview.
    fn tab(
        &self,
        slot: &Slot,
        front: bool,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let workspace = self.workspace.read(cx);
        let renaming = matches!(
            (slot, &self.rename.what),
            (Slot::Space { space }, Some(Renaming::Space(at))) if at == space
        );
        let (key, label, menu) = match slot {
            Slot::Entry(member) => {
                let toolbar = self
                    .root
                    .upgrade()
                    .and_then(|root| root.read(cx).member_toolbar(member, cx));
                // A number is a project's own, and the strip can hold entries
                // of several — so the tab says which project it is counted in.
                let project = workspace
                    .member_project(member)
                    .and_then(|at| workspace.projects.get(at))
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
                    label = label
                        .with_badge(Reference::entry(Some(&project), number, None).to_string());
                }
                let menu = toolbar.is_some_and(|toolbar| toolbar.entry.is_some());
                (arrangement::key_of(member), label, menu)
            }
            Slot::Space { space } => {
                let name = match renaming {
                    true => String::new(),
                    false => workspace
                        .space_by_id(space)
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
                            this.window_menu(
                                |this| &mut this.menu,
                                menu_key.clone(),
                                Place::At(press.position),
                                Self::slot_rows(slot.clone()),
                                window,
                                cx,
                            );
                        }
                    }),
                )
                .children(self.menu_here(
                    &menu_key,
                    "strip-tab-menu-card",
                    window,
                    cx,
                ))
            }
            false => el,
        };
        el.children(renaming.then(|| self.rename.field.clone()))
    }
}

impl MenuHost for Strip {
    type Key = StripMenu;

    fn menu(&self) -> &ContextMenu<Self, StripMenu> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, StripMenu> {
        &mut self.menu
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
        let tracked = self.tracked(cx);
        let global = workspace.settings.appearance.global_tabs();
        // The window's top left is the sidebar's while it is open.
        let corners = chrome::Corners {
            left: !self.sidebar_open,
            right: true,
            tabs: global,
        };
        // The row runs edge to edge; its tabs start where the cards below do.
        let lead = corners.lead(
            match floating {
                true => root::card_gap(floating),
                false => root::HEADER_INSET,
            },
            window,
            cx,
        );
        let grip = match self.root.upgrade() {
            Some(root) => chrome::grip("strip-grip", &root.read(cx).drag, window),
            None => div().into_any_element(),
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
        chrome::top_band(global)
            .id("window-strip")
            .relative()
            .w_full()
            .gap(px(BAR_GAP))
            .pl(px(lead))
            .pr(px(corners.trail(root::BUTTON_EDGE, window, cx)))
            // Under the whole row: any of it no control stands on moves the
            // window, as a titlebar does.
            .child(div().absolute().inset_0().flex().child(grip))
            // The tabs on a track of their own, which scrolls sideways once
            // they no longer fit: the controls keep their places while it does.
            // One tab has no track, back while its name is typed.
            .when(tracked, |el| {
                el.child(
                    self.reorder
                        .bar("window-tabs", &model, tabs)
                        .on_track(match floating {
                            true => tabs::Track::Bare,
                            false => tabs::Track::Flat,
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
            // Empty row kept at the end, so the window can be taken by it
            // however many tabs there are.
            .child(match tracked {
                true => div().flex_none().w(px(DRAG_ROOM)),
                false => div().flex_1(),
            })
    }
}
