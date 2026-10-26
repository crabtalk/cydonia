//! The sidebar's lines: a section's heading, a project's or a space's heading,
//! an entry, the archive's divider, and the rows above and below the list.

use super::{
    BUTTON_SIZE, DETAIL_LINE, EXTENDED_PAD, Group, Heading, MenuRows, PINNED_BLEED, ROW_GAP,
    ROW_HEIGHT, ROW_PILL, Row, SessionRow, Sidebar, SidebarMenu, TITLE_LINE, key_of, on_disk, row,
    row_frame, row_label, tint,
};
use crate::{
    model::{
        project::ProjectId,
        state,
        workspace::{Opening, Showing},
    },
    view::{
        keymap::{self, Command},
        kit::{
            menu::{self, Act, MenuHost},
            new::{self, New},
            rename::Renaming,
            stamp, status, text,
        },
        library::Shelf,
        modals::palette::{Filter, ToggleSearch},
        search::Search,
        window::{
            reach::InWindow as _,
            root::{self, Cydonia, EditAgent, OpenProject},
        },
    },
};
use bezel::{
    agent::{Face, avatar::Motion, mascot},
    gpui::{
        self, AnyElement, App, Context, Div, ElementId, Empty, FontWeight, MouseButton,
        SharedString, Stateful, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{Selection, TextStyle, Theme, Typeset},
    ui::{
        context_menu::Place as MenuPlace,
        icons::{self, Icon},
        menu::Item,
        multi_select, popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Layout},
    },
};
use std::rc::Rc;

/// A line of an extended row under its name: small, muted, cut to one line.
fn detail(text: String, theme: &Theme) -> Div {
    detail_style(div(), theme).min_w_0().truncate().child(text)
}

/// The type of the text under an extended row's name.
fn detail_style(el: Div, theme: &Theme) -> Div {
    el.text_style(TextStyle::Caption)
        .line_height(px(DETAIL_LINE))
        .text_color(theme.text_muted)
}

impl Sidebar {
    /// One line, built when the list scrolls it into view. The box around it is
    /// what holds the pitch: the row inside paints the wash, and the pixel
    /// either side of it is the gap between two.
    pub(super) fn sidebar_row(
        &self,
        row: &Row,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let inner = match row {
            Row::Group(group) => self.group_head(group.clone(), false, lifted, window, cx),
            Row::Entry { .. } => self.entry_row(row, lifted, window, cx),
            Row::App(id) => self.app_row(row, id, lifted, cx),
            Row::Heading(heading) => self.heading_row(*heading, cx),
        };
        let entry = !matches!(
            row,
            Row::Group(Group::Project(_) | Group::Agent(_)) | Row::Heading(_)
        );
        let pinned = row.pinned(self.workspace.read(cx), cx);
        let pitch = self.shape(row, cx).pitch();
        div()
            .id(SharedString::from(format!("sidebar-hover-{}", key_of(row))))
            .when(entry, |el| {
                el.on_hover(cx.listener({
                    let row = row.clone();
                    move |this, hovered: &bool, _, cx| {
                        this.hover(SidebarMenu::Entry(row.clone()), *hovered, cx);
                    }
                }))
                // Every kind of row from one place, and the menu drawn here
                // rather than under whatever the row ends in: a right press
                // lands wherever the pointer is, and the trigger it opens from
                // is the row.
                .relative()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let row = row.clone();
                        move |this, press: &gpui::MouseDownEvent, window, cx| {
                            let row = row.clone();
                            this.toggle_menu(
                                SidebarMenu::Entry(row.clone()),
                                MenuPlace::At(press.position),
                                move |this, _, cx| Self::through(this.entry_rows(&row, cx)),
                                window,
                                cx,
                            );
                        }
                    }),
                )
                .children(
                    (!pinned)
                        .then(|| self.entry_menu(&SidebarMenu::Entry(row.clone()), window, cx))
                        .flatten(),
                )
            })
            .h(px(pitch))
            .py(px(1.))
            .child(inner)
    }

    /// A project's or a space's heading: a press folds it. A space opens from
    /// the entries under it, and its heading is never lit — the lit row is the
    /// entry in the focused pane, see [`super::Lit::light`].
    ///
    /// `pinned` is the copy [`Sidebar::pinned_head`] holds at the top of the
    /// list. It gives up the pill for the column's full width, and takes the
    /// glass the floating cluster below it is cut from — a heading with rows
    /// running under it has to be read against whatever is passing.
    pub(super) fn group_head(
        &self,
        group: Group,
        pinned: bool,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let trail = self.trail(cx);
        let workspace = self.workspace.read(cx);
        let (name, folded, marks, space_id): (String, bool, (Icon, Icon), Option<String>) =
            match &group {
                Group::Agent(id) => match workspace.identity(id) {
                    Some(identity) => (
                        identity.name.clone(),
                        workspace.agent_folded(id),
                        (
                            icons::development::Bot.into(),
                            icons::development::Bot.into(),
                        ),
                        None,
                    ),
                    None => return Empty.into_any_element(),
                },
                Group::Project(path) => match workspace
                    .project_ix(*path)
                    .and_then(|ix| workspace.projects.get(ix))
                {
                    Some(project) => (
                        project.name(),
                        !project.expanded,
                        (icons::files::FolderOpen.into(), icons::files::Folder.into()),
                        None,
                    ),
                    None => return Empty.into_any_element(),
                },
                Group::Space(id) => match workspace.space_by_id(id) {
                    Some(space) => (
                        space.label().to_owned(),
                        workspace.space_folded(&space.id),
                        (
                            icons::layout::LayoutFreeform.into(),
                            icons::layout::LayoutDashboard.into(),
                        ),
                        Some(space.id.clone()),
                    ),
                    None => return Empty.into_any_element(),
                },
            };
        let folded = folded && self.search.read(cx).query().is_none();
        // A space's mark is its most pressing session's, while one is not
        // idle.
        let status = match &group {
            Group::Space(id) => {
                status::Mark::of_most_pressing(self.workspace.read(cx).space_sessions(id, cx))
            }
            Group::Agent(id) => {
                status::Mark::of_most_pressing(self.workspace.read(cx).identity_sessions(id, cx))
            }
            Group::Project(_) => None,
        };
        // An identity wears its blob where a group's mark goes.
        let avatar = match &group {
            Group::Agent(id) => self
                .workspace
                .read(cx)
                .identity(id)
                .map(|identity| identity.avatar),
            Group::Project(_) | Group::Space(_) => None,
        };
        let key = key_of(&Row::Group(group.clone()));
        let (menu, add) = match &group {
            Group::Project(project) => (
                SidebarMenu::Project(*project),
                Some(SidebarMenu::Add(*project)),
            ),
            Group::Space(_) => (SidebarMenu::Entry(Row::Group(group.clone())), None),
            Group::Agent(id) => (SidebarMenu::Agent(id.clone()), None),
        };
        // The buttons stay on show while either one's menu is open. They are
        // revealed by the row's hover, and the pointer leaves the row the
        // moment it reaches the card — which took the `+` away from under a
        // menu standing open beside it.
        let held = self.menu_open(&menu) || add.as_ref().is_some_and(|add| self.menu_open(add));
        let reveal = (!held).then_some("group-head");
        let renaming = match &group {
            Group::Agent(id) => matches!(&self.rename.what, Some(Renaming::Agent(at)) if at == id),
            _ => space_id.as_ref().is_some_and(
                |id| matches!(&self.rename.what, Some(Renaming::Space(at)) if at == id),
            ),
        };
        let label = match renaming {
            true => self.rename.field.clone().into_any_element(),
            false => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Callout)
                .font_weight(FontWeight::MEDIUM)
                .line_height(px(18.))
                .child(name)
                .into_any_element(),
        };
        let menu_rows: MenuRows = match &group {
            Group::Project(project) => {
                let project = *project;
                Rc::new(move |this, cx| this.project_rows(project, cx))
            }
            Group::Space(_) => {
                let row = Row::Group(group.clone());
                Rc::new(move |this, cx| this.entry_rows(&row, cx))
            }
            Group::Agent(id) => {
                let id = id.clone();
                Rc::new(move |this, cx| this.agent_menu_rows(&id, cx))
            }
        };
        let head = div()
            .id(SharedString::from(format!("group-{key}")))
            .group("group-head")
            .on_hover(cx.listener({
                let menu = menu.clone();
                move |this, hovered: &bool, _, cx| {
                    this.hover(menu.clone(), *hovered, cx);
                }
            }))
            // Pinned it runs edge to edge, and past the band it shows in at
            // the top and the bottom — see [`PINNED_BLEED`]. The label and the
            // buttons keep the x the pill's own margin and padding put them at.
            //
            // The right side is an entry row's, so the buttons end where a
            // row's do; the left is 2 short of it, which the fold button's
            // own padding makes up, so its mark stands over the rows' marks.
            .when(pinned, |el| {
                el.absolute()
                    .top(px(-PINNED_BLEED))
                    .left_0()
                    .right_0()
                    .h(px(ROW_HEIGHT + 2. * PINNED_BLEED))
                    .pl(px(root::SIDEBAR_GUTTER + 6.))
                    .pr(px(trail + root::SIDEBAR_GUTTER))
            })
            .when(!pinned, |el| {
                el.relative()
                    .ml(px(root::SIDEBAR_GUTTER))
                    .mr(px(trail))
                    .pl(px(6.))
                    .pr(px(root::SIDEBAR_GUTTER))
                    .h(px(ROW_PILL))
                    .rounded(px(Theme::control_radius()))
            })
            .when(lifted, |el| el.m_0())
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            // On the head, not the label: a name's colour is fixed when its
            // text is laid out, and only this div is stateful enough to carry
            // the hover that far.
            .text_color(theme.text_muted)
            .hover(|el| el.text_color(theme.text))
            .child(
                theme
                    .ghost(SharedString::from(format!("group-fold-{key}")))
                    .flex_none()
                    .p(px(2.))
                    .child(match (status, avatar) {
                        (Some(status), _) => div()
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(status.draw(cx))
                            .into_any_element(),
                        // Asleep while its sessions are folded away.
                        (None, Some(avatar)) => div()
                            .size(px(14.))
                            .child(mascot(
                                &Face::from(avatar.0).motion(match folded {
                                    true => Motion::ASLEEP,
                                    false => Motion::default(),
                                }),
                                0.,
                            ))
                            .into_any_element(),
                        (None, None) => icons::icon(match folded {
                            false => marks.0,
                            true => marks.1,
                        })
                        .size(px(14.))
                        .text_color(theme.text_muted)
                        .group_hover("group-head", |el| el.text_color(theme.text))
                        .into_any_element(),
                    })
                    .on_click(cx.listener({
                        let group = group.clone();
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.fold_group(&group, window, cx);
                        }
                    })),
            )
            .child(label)
            .children(match &group {
                Group::Project(project) => self.label_filter_chip(*project, cx),
                Group::Space(_) | Group::Agent(_) => None,
            })
            // Ahead of the `+`, which is the one that gets pressed: sorting
            // and the rest are settled once and left alone.
            .child(
                self.menu_button(
                    SharedString::from(format!("group-menu-{key}")),
                    reveal,
                    icons::layout::Ellipsis,
                    menu.clone(),
                    MenuPlace::Below,
                    menu_rows.clone(),
                    cx,
                )
                // On the trigger, not the row: the card pins to the bottom
                // left of whatever it is mounted on, and from the row it hangs
                // off the far side of the sidebar rather than under the `···`.
                // A space's is drawn by the row around it — see
                // [`Self::sidebar_row`].
                .children(match &group {
                    Group::Project(project) => self.project_menu(*project, window, cx),
                    Group::Agent(id) => self.menu_here(
                        &SidebarMenu::Agent(id.clone()),
                        SharedString::from(format!("agent-menu-{id}")),
                        window,
                        cx,
                    ),
                    Group::Space(_) => None,
                }),
            )
            .children(match &group {
                Group::Project(project) => Some(
                    self.menu_button(
                        SharedString::from(format!("group-add-{key}")),
                        reveal,
                        icons::math::Plus,
                        SidebarMenu::Add(*project),
                        MenuPlace::Below,
                        Rc::new({
                            let project = *project;
                            move |this, cx| this.add_rows(project, cx)
                        }),
                        cx,
                    )
                    .children(self.add_menu(*project, window, cx)),
                ),
                // Nothing to choose between: a session as the identity.
                Group::Agent(id) => Some(
                    menu::button(
                        SharedString::from(format!("group-add-{key}")),
                        icons::math::Plus,
                        match reveal {
                            Some(group) => menu::Reveal::Group(group),
                            None => menu::Reveal::Always,
                        },
                        false,
                        cx,
                    )
                    .tooltip(|window, cx| Tooltip::text("New session", window, cx))
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, window, cx| {
                            cx.stop_propagation();
                            let id = id.clone();
                            this.defer_window(window, cx, move |root, window, cx| {
                                root.new_session_as(&id, window, cx)
                            });
                        }
                    })),
                ),
                Group::Space(_) => None,
            })
            // A press on the copy is a press on where it came from: the list
            // goes back to the heading it is standing in for, rather than
            // folding away what you are reading. Its own mark still folds —
            // that press stops before it reaches here.
            .on_click(cx.listener({
                let group = group.clone();
                move |this, _, window, cx| match pinned {
                    true => this.scroll_to_group(&group, cx),
                    false => this.fold_group(&group, window, cx),
                }
            }));
        // A project's menu opens on the press rather than the click, so the
        // note has to be here too — read stale, a right press would swallow.
        // A space's opens from the row around it, as an entry's does.
        let head = match &group {
            Group::Project(project) => self
                .menu_trigger(head, SidebarMenu::Project(*project), cx)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let project = *project;
                        move |this, press: &gpui::MouseDownEvent, window, cx| {
                            this.toggle_menu(
                                SidebarMenu::Project(project),
                                MenuPlace::At(press.position),
                                move |this, _, cx| Self::through(this.project_rows(project, cx)),
                                window,
                                cx,
                            );
                        }
                    }),
                ),
            Group::Agent(id) => self
                .menu_trigger(head, SidebarMenu::Agent(id.clone()), cx)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let id = id.clone();
                        move |this, press: &gpui::MouseDownEvent, window, cx| {
                            let id = id.clone();
                            this.toggle_menu(
                                SidebarMenu::Agent(id.clone()),
                                MenuPlace::At(press.position),
                                move |this, _, cx| Self::through(this.agent_menu_rows(&id, cx)),
                                window,
                                cx,
                            );
                        }
                    }),
                ),
            Group::Space(_) => head,
        };
        match pinned {
            // The same token the cluster at the foot of the column mounts on,
            // so the two glasses in the sidebar move together.
            true => head
                .surface(&theme, theme.popover_surface)
                .into_any_element(),
            false => head.into_any_element(),
        }
    }

    /// What the sidebar needs of a session, read when its row comes on screen.
    fn session_of(&self, project: usize, id: u64, cx: &App) -> Option<SessionRow> {
        let workspace = self.workspace.read(cx);
        let chat = workspace.projects.get(project)?.session(id, cx)?;
        Some(SessionRow {
            id: chat.id,
            icon: workspace.agent_icon(&chat.entry.name),
            status: status::Mark::of(chat),
        })
    }

    /// One entry: its mark, its name, and the button at its end. A press
    /// opens it.
    fn entry_row(
        &self,
        entry: &Row,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Row::Entry { showing, .. } = entry else {
            return Empty.into_any_element();
        };
        let theme = Theme::of(cx).clone();
        let trail = self.trail(cx);
        let workspace = self.workspace.read(cx);
        let Some((project, ix)) = entry.located(workspace) else {
            return Empty.into_any_element();
        };
        let light = self.lit.light(entry, workspace, cx);
        let selected = light.selected();
        let facts = workspace.projects[project].facts_at(showing.kind(), ix, cx);
        let preview = self
            .extended(cx)
            .then(|| workspace.projects[project].preview_at(showing.kind(), ix, cx));
        let archived = facts.archived;
        let tint = light.tint(archived, &theme);
        let glyph = |icon: Icon| {
            icons::icon(icon)
                .size(px(14.))
                .flex_none()
                .text_color(tint)
                .into_any_element()
        };
        let (id, group, mark): (ElementId, &'static str, AnyElement) = match showing {
            Showing::Session(id) => {
                let Some(session) = self.session_of(project, *id, cx) else {
                    return Empty.into_any_element();
                };
                // The agent's own mark, in the label's colour rather than any
                // of its own: every icon the registry publishes is a
                // `currentColor` glyph, so tinting is the only colour it will
                // ever have. While it is not idle its status's mark stands in
                // its place.
                let mark = match session.status {
                    // An orb is wider than the slot it sits in, and left to
                    // spill: widening the column for it would move every label
                    // in the sidebar to make room for a row that is only
                    // sometimes working.
                    Some(status) => status.draw(cx),
                    None => match session.icon {
                        Some(icon) => icons::icon(icon)
                            .size(px(14.))
                            .text_color(tint)
                            .into_any_element(),
                        None => Empty.into_any_element(),
                    },
                };
                let mark = div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark)
                    .into_any_element();
                (("session", session.id).into(), "session-row", mark)
            }
            Showing::Board(_) => (
                SharedString::from(key_of(entry)).into(),
                "board-row",
                glyph(icons::development::SquareKanban.into()),
            ),
            Showing::Article(_) => (
                SharedString::from(key_of(entry)).into(),
                "article-row",
                glyph(icons::files::FileText.into()),
            ),
            Showing::Table(_) => (
                SharedString::from(key_of(entry)).into(),
                "table-row",
                glyph(icons::files::Table2.into()),
            ),
        };
        // An article is named at the head of its own page and a board by its
        // identity panel, so neither row is ever a field.
        let label =
            match Renaming::of(entry).is_some_and(|at| self.rename.what.as_ref() == Some(&at)) {
                true => self.rename.field.clone().into_any_element(),
                false => row_label(facts.label, tint),
            };
        let open = entry.clone();
        let button = self.archive_button(
            format!("archive-{}", key_of(entry)),
            group,
            entry,
            archived,
            window,
            cx,
        );
        let frame = row(
            id,
            group,
            Selection::new(selected, self.emphasized),
            lifted,
            self.list(cx).indent_of(entry),
            trail,
            &theme,
        );
        let pill = self.shape(entry, cx).pill();
        let frame = match preview {
            None => frame.child(mark).child(label).child(button),
            // The lines under the name run the row's whole width, from under
            // the mark: indented under the name they would be a column too
            // narrow to say anything.
            Some(preview) => frame
                .h(px(pill))
                .py(px(EXTENDED_PAD))
                .flex_col()
                .items_start()
                .gap_0()
                .child(
                    div()
                        .w_full()
                        .h(px(TITLE_LINE))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(ROW_GAP))
                        .child(mark)
                        .child(label)
                        .child(button),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(DETAIL_LINE))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(ROW_GAP))
                        .child(
                            detail(stamp::ago(facts.touched), &theme)
                                .flex_none()
                                .text_color(tint),
                        )
                        .when(!preview.is_empty(), |line| {
                            line.child(detail(preview, &theme).flex_1())
                        }),
                )
                .when(!facts.labels.is_empty(), |el| {
                    el.child(
                        div()
                            .w_full()
                            .h(px(DETAIL_LINE))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.))
                            .overflow_hidden()
                            .children(facts.labels.iter().map(|label| {
                                theme
                                    .chip(label.clone(), multi_select::tint(&theme, label))
                                    .text_style(TextStyle::Caption)
                                    .line_height(px(DETAIL_LINE))
                            })),
                    )
                }),
        };
        frame
            .on_click(cx.listener(move |this, _, window, cx| {
                let open = open.clone();
                this.defer_window(window, cx, move |root, window, cx| {
                    root.open_row(&open, Opening::Preview, window, cx)
                });
            }))
            .into_any_element()
    }

    /// One app: its mark, its name, and whether it is built on this machine.
    /// A press opens it.
    fn app_row(&self, entry: &Row, id: &str, lifted: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let trail = self.trail(cx);
        let workspace = self.workspace.read(cx);
        let Some(program) = workspace.program(id, cx) else {
            return Empty.into_any_element();
        };
        let program = program.read(cx);
        let light = self.lit.light(entry, workspace, cx);
        let tint = light.tint(false, &theme);
        let open = entry.clone();
        row(
            SharedString::from(key_of(entry)),
            "app-row",
            Selection::new(light.selected(), self.emphasized),
            lifted,
            self.list(cx).indent_of(entry),
            trail,
            &theme,
        )
        .child(
            icons::icon(crate::view::kit::icon::kind_icon(artifact::kind::Kind::App))
                .size(px(14.))
                .flex_none()
                .text_color(tint),
        )
        .child(row_label(program.label(), tint))
        .children(program.local().then(|| theme.badge("Local")))
        .on_click(cx.listener(move |this, _, window, cx| {
            let open = open.clone();
            this.defer_window(window, cx, move |root, window, cx| {
                root.open_row(&open, Opening::Preview, window, cx)
            });
        }))
        .into_any_element()
    }

    /// The button at the end of a row: archive, or — for a pinned row — the
    /// pin it is marked with, which opens the row's menu.
    ///
    /// A pin is the one state a row carries that nothing else on it shows, so
    /// it is drawn at rest rather than on hover. Under the pointer the same
    /// button becomes the `···`, and what a pinned row can have done to it is
    /// in the menu rather than behind a press that has to mean one of them.
    fn archive_button(
        &self,
        id: impl Into<SharedString>,
        group: &'static str,
        entry: &Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let id = id.into();
        if entry.pinned(self.workspace.read(cx), cx) {
            let at = SidebarMenu::Entry(entry.clone());
            // The glyph alone turns over; the button is the same button either
            // way, so a press during the frame the hover is still travelling
            // opens the menu rather than falling through to the row.
            let open = self.menu_open(&at);
            let mark = match open || self.hovered.as_ref() == Some(&at) {
                true => icons::layout::Ellipsis,
                false => icons::navigation::Pin,
            };
            let row = entry.clone();
            return self
                .menu_button(
                    id,
                    None,
                    mark,
                    at.clone(),
                    MenuPlace::BelowEnd,
                    Rc::new(move |this, cx| this.entry_rows(&row, cx)),
                    cx,
                )
                // On the trigger, so the card hangs under the `···` rather
                // than off the left edge of the row it is mounted on. A row
                // with no button of its own draws it from the wrapper, where
                // a right press is all there is to anchor to.
                .children(self.entry_menu(&at, window, cx));
        }
        let held = matches!(self.menu.key(), Some(SidebarMenu::Entry(at)) if at == entry);
        let entry = entry.clone();
        let mark = match archived {
            true => icons::files::ArchiveRestore,
            false => icons::files::Archive,
        };
        theme
            .icon_button(
                mark,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), id.clone())),
            )
            .id(id)
            .flex_none()
            // Out of sight but laid out, and revealed off the row's own hover
            // group rather than [`Sidebar::hovered`].
            //
            // A group resolves inside the frame the pointer arrives on; a
            // field read at render is a frame behind, because the hover has to
            // go through the model and come back as a repaint. An element out
            // of sight registers no mouse handler either way, so that frame is
            // one in which the button cannot be pressed — and a press landing
            // in it goes to the row instead and reads as a click that did
            // nothing.
            // Held open while its row's menu is: the pointer leaves the row
            // the moment it reaches the card, and a button that collapsed
            // then would take the row's shape with it.
            .when(!held, |el| {
                el.invisible()
                    .w(px(0.))
                    .ml(px(-ROW_GAP))
                    .group_hover(group, |el| el.visible().w(px(BUTTON_SIZE)).ml(px(0.)))
            })
            // And taking no width until then. Laid out at its full size the
            // button is a column down the whole list, holding space nothing is
            // in and truncating every name by what an archive glyph would take
            // — which is only ever wanted under the pointer. The row's `gap`
            // still falls either side of a child with no width, so the margin
            // that cancels it comes back with the width.
            .overflow_hidden()
            .tooltip(move |window, cx| {
                Tooltip::text(
                    match archived {
                        true => "Unarchive",
                        false => "Archive",
                    },
                    window,
                    cx,
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                let entry = entry.clone();
                this.defer_window(window, cx, move |root, window, cx| {
                    root.archive_entry(&entry, !archived, window, cx)
                });
            }))
    }

    /// An entry's menu, while `at` is the one up. A right press opens it at the
    /// pointer; a button — the pin — below its own right edge.
    fn entry_menu(
        &self,
        at: &SidebarMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.menu_here(at, "sidebar-entry-menu", window, cx)
    }

    /// A section's heading: a press folds the section, and the chevron beside
    /// the label, shown while the pointer is on the heading, says which way it
    /// stands. The projects' also holds the way to open another, shown while
    /// the pointer is on the heading.
    fn heading_row(&self, heading: Heading, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let trail = self.trail(cx);
        let workspace = self.workspace.read(cx);
        let folded =
            self.search.read(cx).query().is_none() && workspace.section_folded(heading.key());
        let group: &'static str = match heading {
            Heading::Projects => "projects-heading",
            Heading::Agents => "agents-heading",
            Heading::Spaces => "spaces-heading",
            Heading::Apps => "apps-heading",
        };
        // The square every `···` and `+` in this column is, in the heading's
        // ink.
        let plus = |id: &'static str| {
            theme
                .tinted_icon_button(icons::math::Plus, theme.text_faint)
                .id(id)
                .flex_none()
                .invisible()
                .group_hover(group, |el| el.visible())
        };
        let open = match heading {
            Heading::Projects => {
                let chord = keymap::label(Command::OpenProject, &workspace.settings.shortcuts);
                Some(
                    plus("open-project")
                        .tooltip(move |window, cx| match chord.clone() {
                            Some(chord) => {
                                Tooltip::with_keystroke("Open project", chord, window, cx)
                            }
                            None => Tooltip::text("Open project", window, cx),
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.defer_window(window, cx, |root, window, cx| {
                                root.open_project_action(&OpenProject, window, cx)
                            });
                        })),
                )
            }
            Heading::Agents => Some(
                plus("new-agent")
                    .tooltip(|window, cx| Tooltip::text("New agent", window, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.defer_window(window, cx, |root, window, cx| {
                            root.edit_agent_action(
                                &EditAgent {
                                    id: None,
                                    from: None,
                                },
                                window,
                                cx,
                            )
                        });
                    })),
            ),
            Heading::Spaces | Heading::Apps => None,
        };
        // A row's frame without its washes, the label ahead of its chevron.
        row_frame(group, group, 0, trail)
            .text_color(theme.text_faint)
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .child(heading.label()),
            )
            .child(
                div()
                    .flex_none()
                    .invisible()
                    .group_hover(group, |el| el.visible())
                    .child(theme.disclosure(!folded).text_color(theme.text_faint)),
            )
            .child(div().flex_1())
            .children(open)
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.search.read(cx).query().is_some() {
                    return;
                }
                this.workspace.update(cx, |workspace, cx| {
                    workspace.toggle_section(heading.key(), cx)
                });
            }))
            .into_any_element()
    }

    /// What the `+` makes here. No session.
    fn add_rows(&self, project: ProjectId, cx: &App) -> Vec<(Item, Act)> {
        let workspace = self.workspace.read(cx);
        let Some(ix) = workspace.project_ix(project) else {
            return Vec::new();
        };
        let offered = new::Offered {
            sessions: false,
            ..new::offered(workspace)
        };
        new::rows(offered, |kind| {
            Box::new(move |this: &mut Cydonia, window, cx| match kind {
                New::Session(_) => {}
                New::Board => this.ask_new_board(ix, window, cx),
                New::Article => this.new_article(ix, window, cx),
                New::Table => this.new_table(ix, window, cx),
            })
        })
    }

    /// The `+` menu of `project`, while it is up.
    fn add_menu(
        &self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ix = self.workspace.read(cx).project_ix(project)?;
        let id = SharedString::from(format!("add-menu-{ix}"));
        self.menu_here(&SidebarMenu::Add(project), id, window, cx)
    }

    /// The label a project's list is narrowed to, on its heading, with a ×
    /// that lists every entry again.
    fn label_filter_chip(&self, project: ProjectId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let workspace = self.workspace.read(cx);
        let ix = workspace.project_ix(project)?;
        let label = workspace.projects[ix].label.clone()?;
        let theme = Theme::of(cx).clone();
        Some(
            theme
                .tag(label)
                .self_center()
                .flex_none()
                .id(SharedString::from(format!("group-label-{ix}")))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.set_project_label(ix, None, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    /// The project menu's "Filter by label": the labels its entries carry,
    /// one at a time. Nothing for a project with none.
    fn label_filter_menu(&self, ix: usize, cx: &App) -> Option<(Item, Act)> {
        let workspace = self.workspace.read(cx);
        let counts = workspace.project_label_counts(ix, cx);
        let picked = workspace.projects.get(ix)?.label.clone();
        if counts.is_empty() && picked.is_none() {
            return None;
        }
        let pick = |label: Option<String>, item: Item| {
            menu::row::<Cydonia>(item, move |this, _, cx| {
                let label = label.clone();
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_project_label(ix, label, cx)
                });
            })
        };
        let mut rows = vec![pick(None, Item::action("None").checked(picked.is_none()))];
        rows.extend(counts.into_iter().map(|(label, count)| {
            let item = Item::action(label.clone())
                .checked(picked.as_deref() == Some(label.as_str()))
                .with_description(text::count(count, "entry", "entries"));
            pick(Some(label), item)
        }));
        Some(menu::submenu(
            "Filter by label",
            icons::text::ListFilter,
            rows,
        ))
    }

    /// What a press on the heading opens. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_rows(&self, project: ProjectId, cx: &App) -> Vec<(Item, Act)> {
        let workspace = self.workspace.read(cx);
        let Some(ix) = workspace.project_ix(project) else {
            return Vec::new();
        };
        let sort = workspace.sort_of(ix);
        let by = |label: &'static str, mode: state::Sort| {
            menu::row::<Cydonia>(
                Item::action(label).checked(sort == mode),
                move |this, _, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.set_sort(ix, mode, cx));
                },
            )
        };
        let mut rows = vec![menu::submenu(
            "Sort by",
            icons::text::ArrowDownAZ,
            vec![
                by("Name", state::Sort::Name),
                by("Last modified", state::Sort::Touched),
                // Last, and named for what it is: the other two are
                // orders nobody arranged, and this is the one that is.
                by("Manual", state::Sort::Manual),
            ],
        )];
        rows.extend(self.label_filter_menu(ix, cx));
        rows.push(menu::row::<Cydonia>(
            Item::action("Show archived").with_icon(icons::files::Archive),
            move |this, window, cx| this.open_library(Shelf::archived_in(project), window, cx),
        ));
        rows.extend(on_disk(workspace.path_of(project).to_path_buf()));
        rows.push(menu::row::<Cydonia>(
            Item::action("Remove project").with_icon(icons::files::FolderMinus),
            move |this, _, cx| this.close_project(ix, cx),
        ));
        rows
    }

    /// What an identity's `···` holds.
    fn agent_menu_rows(&self, id: &str, cx: &App) -> Vec<(Item, Act)> {
        let Some(name) = self
            .workspace
            .read(cx)
            .identity(id)
            .map(|identity| identity.name.clone())
        else {
            return Vec::new();
        };
        let (edit, rename, remove) = (id.to_owned(), id.to_owned(), id.to_owned());
        vec![
            menu::row::<Cydonia>(
                Item::action("Edit agent…").with_icon(icons::development::Bot),
                move |root, window, cx| {
                    let edit = EditAgent {
                        id: Some(edit.clone()),
                        from: None,
                    };
                    root.edit_agent_action(&edit, window, cx)
                },
            ),
            menu::row::<Cydonia>(
                Item::action("Rename").with_icon(icons::text::SquarePen),
                move |root, window, cx| {
                    let what = Renaming::Agent(rename.clone());
                    root.sidebar
                        .update(cx, |sidebar, cx| sidebar.start_rename(what, window, cx));
                },
            ),
            menu::row::<Cydonia>(
                Item::action("Remove").with_icon(icons::files::Trash),
                move |root, window, cx| {
                    root.ask_delete_identity(remove.clone(), name.clone(), window, cx)
                },
            ),
        ]
    }

    /// The heading's menu, while it is up: at the pointer from a right press,
    /// under the `···` from the button.
    fn project_menu(
        &self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ix = self.workspace.read(cx).project_ix(project)?;
        let id = SharedString::from(format!("project-menu-{ix}"));
        self.menu_here(&SidebarMenu::Project(project), id, window, cx)
    }

    /// The row that raises the palette, or — while a query is applied — the
    /// query, how many it found, and the × that clears it.
    pub(super) fn search_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let trail = self.trail(cx);
        let chord = keymap::label(Command::Search, &self.workspace.read(cx).settings.shortcuts);
        let raise = cx.listener(|this, _, window, cx| {
            this.defer_window(window, cx, |root, window, cx| {
                root.toggle_search(&ToggleSearch, window, cx)
            });
        });
        let search = self.search.read(cx);
        if let Some((text, filter, ready)) = search.shown() {
            let label = match filter {
                Some(kind) => format!("{text} · {}", Filter::Kind(kind).label()),
                None => text,
            };
            let count = search.rows(cx).map_or(0, |rows| rows.len());
            return row(
                "applied-search",
                "applied-search",
                Selection::None,
                false,
                0,
                trail,
                &theme,
            )
            .flex_none()
            .min_w_0()
            .mb(px(4.))
            .child(
                icons::icon(icons::text::Search)
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Body)
                    .text_color(theme.text)
                    .child(label),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(match ready {
                        false => "…".to_owned(),
                        true => count.to_string(),
                    }),
            )
            .child(
                theme
                    .ghost("clear-workspace-search")
                    .debug_selector(|| "clear-workspace-search".into())
                    .flex_none()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icons::icon(icons::notifications::X)
                            .size(px(14.))
                            .text_color(theme.text_muted),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.search.update(cx, Search::clear);
                    })),
            )
            .on_click(raise)
            .into_any_element();
        }
        row(
            "search-row",
            "search-row",
            Selection::None,
            false,
            0,
            trail,
            &theme,
        )
        .flex_none()
        .mb(px(4.))
        .child(
            icons::icon(icons::text::Search)
                .size(px(14.))
                .text_color(theme.text_muted),
        )
        .child(
            div()
                .flex_1()
                .text_style(TextStyle::Body)
                .text_color(theme.text_muted)
                .child("Search"),
        )
        .children(chord.map(|chord| popover::kbd_hint(&theme, chord)))
        .on_click(raise)
        .into_any_element()
    }

    /// The row that puts the library up, lit while it is.
    pub(super) fn library_entry(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let open = self.lit.library;
        let tone = tint(open, false, &theme);
        let trail = self.trail(cx);
        row(
            "library-entry",
            "library-entry",
            Selection::new(open, self.emphasized),
            false,
            0,
            trail,
            &theme,
        )
        .flex_none()
        .mb(px(4.))
        .child(
            icons::icon(icons::text::LibraryBig)
                .size(px(14.))
                .text_color(tone),
        )
        .child(
            div()
                .flex_1()
                .text_style(TextStyle::Body)
                .text_color(tone)
                .child("Library"),
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.defer_window(window, cx, |root, window, cx| {
                root.toggle_library(window, cx)
            });
        }))
        .into_any_element()
    }

    /// The line at the foot of the column: the gear that opens the settings
    /// window.
    pub(super) fn settings_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let chord = keymap::label(
            Command::OpenSettings,
            &self.workspace.read(cx).settings.shortcuts,
        );
        // Inset as a row is, so the button ends where the rows' buttons end.
        div()
            .flex_none()
            .h(px(ROW_PILL))
            .ml(px(root::SIDEBAR_GUTTER))
            .mr(px(self.trail(cx)))
            .mb(px(root::SIDEBAR_GUTTER))
            .px(px(root::SIDEBAR_GUTTER))
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .child(
                theme
                    .icon_button(
                        icons::account::Settings,
                        ButtonStyle::Ghost,
                        Some(Fade::new(Painter::of(cx), "settings")),
                    )
                    .id("settings")
                    // What it opens is said in the tooltip. The chord is read
                    // off the table the keymap was built from rather than
                    // typed beside the label: it is the reader's to move.
                    .tooltip(move |window, cx| match chord.clone() {
                        Some(chord) => Tooltip::with_keystroke("Settings", chord, window, cx),
                        None => Tooltip::text("Settings", window, cx),
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.defer_window(window, cx, |root, window, cx| {
                            root.open_settings(None, window, cx)
                        });
                    })),
            )
    }

    /// The one place outside settings that says a release is in hand: a line at
    /// the foot of the sidebar, over the controls, that restarts into it.
    ///
    /// Nothing shows here until a bundle is staged and verified — see
    /// [`crate::model::update`], and the Developer section for the switch that
    /// puts it on screen without one.
    #[cfg(not(feature = "desktop"))]
    pub(super) fn restart_notice(&self, cx: &mut Context<Self>) -> Option<Empty> {
        let _ = cx;
        None
    }

    #[cfg(feature = "desktop")]
    pub(super) fn restart_notice(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let updater = crate::model::update::of(cx)?;
        let version = updater.read(cx).ready()?;
        let theme = Theme::of(cx).clone();
        let ready = SharedString::from(format!("cydonia {version} is ready"));
        Some(
            theme
                .ghost("restart-to-update")
                .flex_none()
                .mx(px(8.))
                .mb(px(8.))
                .px(px(8.))
                .py(px(6.))
                .gap(px(6.))
                // No plate under it: `ghost` paints one on hover, and anything
                // at rest would have to be quieter than that to leave the hover
                // anything to say. What gives the line its weight is the mark,
                // which is the only accent-coloured thing in the column.
                .tooltip(move |window, cx| Tooltip::text(ready.clone(), window, cx))
                .child(
                    icons::icon(icons::development::CircleFadingArrowUp)
                        .size(px(13.))
                        .flex_none()
                        .text_color(theme.accent),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::Footnote)
                        .child("Restart to update"),
                )
                .child(theme.badge(version))
                .on_click(cx.listener(move |_, _, _, cx| {
                    updater.update(cx, |updater, cx| updater.restart(cx));
                })),
        )
    }
}
