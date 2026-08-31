//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it. The window's grid lives in [`crate::view::root`];
//! this draws on it.

use crate::{
    model::session::ChatSession,
    view::{
        component::menu::{self, Menu},
        root::{self, CommitName, Cydonia, DismissName, NewSession, OpenProject, Pane},
        settings::Section,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, Context, Div, Empty, Focusable as _, FontWeight, Hsla, MouseButton,
        SharedString, Stateful, Window, div, prelude::*, px, svg,
    },
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, loaders,
        menu::Item,
        popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::{Buttons, Layout},
    },
};

/// What the sidebar needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    id: u64,
    label: String,
    icon: Option<SharedString>,
    streaming: bool,
    archived: bool,
}

/// What the sidebar's name field is attached to. One field for both, because
/// only one row can be being named at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Renaming {
    Session(u64),
    Board { project: usize, ix: usize },
}

/// The box every row under a project heading sits in: indented beneath the
/// heading, and carrying the wash that says which one is open.
///
/// Shared because the indent is a measurement three files have to agree on.
/// Written out in each of them, it drifts.
pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    selected: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .ml(px(root::SIDEBAR_GUTTER))
        .mr(px(root::SIDEBAR_GUTTER))
        .px(px(root::SIDEBAR_GUTTER))
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
        .when(selected, |el| el.bg(theme.element_active))
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
}

/// The plate's own inset around the control it holds.
const CLUSTER_PAD: f32 = 2.;

/// The floating cluster's height, half of which is the pill's radius: a ghost
/// button's box — a 14pt glyph in 4pt of padding — inside that inset.
const CLUSTER_HEIGHT: f32 = 14. + 2. * 4. + 2. * CLUSTER_PAD;

/// A row's name. The line height is what the field pins itself to: left to
/// gpui's default the label's box is φ×13, and renaming would resize the row
/// under the name being typed.
fn row_label(name: String, tint: Hsla) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_style(TextStyle::Body)
        .line_height(px(18.))
        .text_color(tint)
        .child(name)
        .into_any_element()
}

impl Cydonia {
    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let count = self.workspace.read(cx).projects.len();
        let sections: Vec<AnyElement> = (0..count).map(|ix| self.project_section(ix, cx)).collect();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(root::sidebar_bg(&theme))
            // Drawn ON the column, not left as a gap between two: a bare strip
            // between them would be raw desktop at full strength, a bright line
            // the height of the window.
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            // Both controls out at the trailing edge, the fold last: the
            // lights float in the leading half of the strip, which is what
            // leaves nothing there to pad them clear of.
            .child(
                div()
                    .flex_none()
                    .h(px(root::HEADER_HEIGHT))
                    .pr(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .gap(px(4.))
                    .child(
                        theme
                            .ghost("open-project")
                            .p(px(4.))
                            .tooltip(|window, cx| {
                                Tooltip::with_keystroke("New project", "⌘O", window, cx)
                            })
                            .child(
                                icons::icon(icons::PLUS)
                                    .size(px(14.))
                                    .text_color(theme.text_faint),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_project_action(&OpenProject, window, cx);
                            })),
                    )
                    .child(self.fold_toggle(theme.text_faint, cx)),
            )
            .child(
                div()
                    .id("project-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .children(sections),
            )
            .child(
                theme
                    .ghost("settings")
                    .flex_none()
                    .mx(px(8.))
                    .mb(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .gap(px(8.))
                    .child(
                        icons::icon(icons::SETTINGS_MINIMALISTIC)
                            .size(px(13.))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::Body)
                            .text_color(theme.text_muted)
                            .child("Settings"),
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| this.open_settings(Section::Appearance, cx)),
                    ),
            )
    }

    /// The fold toggle once the sidebar is away, as a glass pill over the
    /// content. Out of flow and hugging the one control it holds: a band would
    /// take a row off every pane to carry a single button, and the column under
    /// it is what the button is for. Only the fold — adding a project acts on
    /// the list you are looking at, and with the list gone it is chrome for
    /// somewhere you are not. Its tone is the strong one, because the plate
    /// floats over whatever the pane shows, which can be a picture we did not
    /// choose.
    pub(crate) fn fold_cluster(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .absolute()
            .top(px((root::HEADER_HEIGHT - CLUSTER_HEIGHT) / 2.))
            // Full screen takes the lights away, and the room they needed
            // would be left as a hole.
            .left(px(if window.is_fullscreen() {
                root::HEADER_INSET
            } else {
                root::TOOLBAR_INSET
            }))
            .h(px(CLUSTER_HEIGHT))
            .p(px(CLUSTER_PAD))
            .rounded(px(CLUSTER_HEIGHT / 2.))
            .flex()
            .flex_row()
            .items_center()
            .child(self.fold_toggle(theme.text, cx))
            // The same glass bezel's own floating bar mounts on. Its
            // `control_bar` is the shipped container, and it refuses this case
            // on purpose: a fixed 56pt tall, and sized by its caller rather
            // than by what it holds.
            .surface(&theme, theme.popover_surface)
            .into_any_element()
    }

    /// The control that folds the sidebar away and brings it back. It belongs
    /// to whichever column runs along the window's left edge, so it changes
    /// strip across the collapse — and takes that strip's tone with it.
    fn fold_toggle(&self, tint: Hsla, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let label = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        theme
            .ghost("toggle-sidebar")
            .p(px(4.))
            .tooltip(move |window, cx| Tooltip::text(label, window, cx))
            .child(
                icons::icon(icons::SIDEBAR_MINIMALISTIC_LEFT)
                    .size(px(14.))
                    .text_color(tint),
            )
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    /// One project in the sidebar: a heading that folds, and everything in the
    /// project under it.
    fn project_section(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(project) = workspace.projects.get(ix) else {
            return Empty.into_any_element();
        };
        let expanded = project.expanded;
        let name = project.name();
        let sessions: Vec<SessionRow> = project
            .ordered()
            .map(|chat| SessionRow {
                id: chat.id,
                label: chat.label(),
                icon: workspace.agent_icon(&chat.entry.name),
                streaming: chat.streaming,
                archived: chat.closed,
            })
            .collect();
        let boards: Vec<(usize, String)> = project
            .boards
            .iter()
            .enumerate()
            .map(|(n, board)| (n, board.label().to_owned()))
            .collect();
        let articles: Vec<(usize, String)> = project
            .articles
            .iter()
            .enumerate()
            .map(|(n, article)| (n, article.label().to_owned()))
            .collect();
        let tables: Vec<(usize, String)> = project
            .tables
            .iter()
            .enumerate()
            .map(|(n, table)| (n, table.name.clone()))
            .collect();

        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .id(("project", ix))
                    .group("project-head")
                    .mx(px(8.))
                    .px(px(6.))
                    .py(px(4.))
                    .rounded(px(Theme::control_radius()))
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .cursor_pointer()
                    // On the head, not the label: a name's colour is fixed when
                    // its text is laid out, and only this div is stateful enough
                    // to carry the hover that far.
                    .text_color(theme.text_faint)
                    .hover(|el| el.text_color(theme.text))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_style(TextStyle::Callout)
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
                    .child(
                        self.menu_button(
                            ("project-add", ix),
                            "project-head",
                            icons::icon(icons::PLUS)
                                .size(px(12.))
                                .text_color(theme.text_faint)
                                .group_hover("project-head", |el| el.text_color(theme.text)),
                            Menu::Add(ix),
                            cx,
                        )
                        .children(self.add_menu(ix, cx)),
                    )
                    .child(
                        div()
                            .id(("project-fold", ix))
                            .flex_none()
                            .rounded(px(Theme::control_radius()))
                            .p(px(3.))
                            .cursor_pointer()
                            .invisible()
                            .group_hover("project-head", |el| el.visible())
                            .child(
                                theme
                                    .disclosure(expanded)
                                    .text_color(theme.text_faint)
                                    .group_hover("project-head", |el| el.text_color(theme.text)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_project(ix, cx);
                            })),
                    )
                    .children(self.project_menu(ix, cx))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _, _, cx| this.toggle_menu(Menu::Project(ix), cx)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_project(ix, cx))),
            )
            .when(expanded, |section| {
                section
                    .children(sessions.into_iter().map(|row| self.session_row(row, cx)))
                    .children(
                        boards
                            .into_iter()
                            .map(|(n, name)| self.board_row(ix, n, name, cx)),
                    )
                    .children(
                        articles.into_iter().map(|(n, title)| {
                            self.article_row(ix, n, title, cx).into_any_element()
                        }),
                    )
                    .children(
                        tables
                            .into_iter()
                            .map(|(n, name)| self.table_row(ix, n, name, cx).into_any_element()),
                    )
            })
            .into_any_element()
    }

    fn toggle_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(project) = workspace.projects.get_mut(ix) {
                project.expanded = !project.expanded;
            }
            cx.notify();
        });
    }

    /// What the `+` starts here. Session first: it is what the sidebar is for.
    fn add_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Add(ix)) {
            return None;
        }
        let rows = vec![
            menu::row(
                Item::action("New session").with_icon(icons::CHAT_ROUND_LINE),
                move |this, window, cx| {
                    this.select_project(ix, cx);
                    this.new_session_action(&NewSession, window, cx);
                },
            ),
            menu::row(
                Item::action("New board").with_icon(icons::LIST),
                move |this, _, cx| this.new_board(ix, cx),
            ),
            menu::row(
                Item::action("New article").with_icon(icons::DOCUMENT_ADD),
                move |this, window, cx| this.new_article(ix, window, cx),
            ),
            menu::row(
                Item::action("New table").with_icon(icons::WIDGET),
                move |this, _, cx| this.new_table(ix, cx),
            ),
        ];
        let id = SharedString::from(format!("add-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// What a press on the heading opens. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Project(ix)) {
            return None;
        }
        let rows = vec![menu::row(
            Item::action("Remove project").with_icon(icons::TRASH_BIN_MINIMALISTIC),
            move |this, _, cx| this.close_project(ix, cx),
        )];
        let id = SharedString::from(format!("project-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// One session: its mark and its name.
    fn session_row(&self, session: SessionRow, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let id = session.id;
        let selected =
            self.showing(cx) == Pane::Chat && self.workspace.read(cx).active_id() == Some(id);
        // An archived session reads a step back; the tint is all that says so.
        let tint = match (selected, session.archived) {
            (true, _) => theme.text,
            (false, true) => theme.text_faint,
            (false, false) => theme.text_muted,
        };
        // The agent's own mark, in the label's colour rather than any of its
        // own: every icon the registry publishes is a `currentColor` glyph, so
        // tinting is the only colour it will ever have. While a turn is in
        // flight the orb stands in its place — the same one the transcript
        // works under.
        let mark = if session.streaming {
            loaders::orb(
                loaders::Orb::Cluster,
                SharedString::from(format!("session-orb-{id}")),
                14.,
                &theme,
                painter,
                cx,
            )
            .into_any_element()
        } else {
            match session.icon {
                Some(path) => svg()
                    .path(path)
                    .size(px(14.))
                    .flex_none()
                    .text_color(tint)
                    .into_any_element(),
                None => Empty.into_any_element(),
            }
        };

        let label = match self.renaming == Some(Renaming::Session(id)) {
            true => self.name_field(cx),
            false => row_label(session.label, tint),
        };

        row(("session", id), "session-row", selected, &theme)
            .py(px(6.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark),
            )
            .child(label)
            .child(
                self.menu_button(
                    ("session-menu", id),
                    "session-row",
                    icons::icon(icons::MENU_DOTS)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Session(id),
                    cx,
                )
                .children(self.session_menu(id, session.archived, cx)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(id, cx);
            }))
            .into_any_element()
    }

    /// One board: its mark and its name.
    fn board_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = self.showing(cx) == Pane::Board
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.board == Some(ix));
        let tint = if selected {
            theme.text
        } else {
            theme.text_muted
        };
        let label = match self.renaming == Some(Renaming::Board { project, ix }) {
            true => self.name_field(cx),
            false => row_label(name, tint),
        };

        row(
            SharedString::from(format!("board-{project}-{ix}")),
            "board-row",
            selected,
            &theme,
        )
        .py(px(6.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .child(
            icons::icon(icons::LIST)
                .size(px(14.))
                .flex_none()
                .text_color(tint),
        )
        .child(label)
        .child(
            self.menu_button(
                SharedString::from(format!("board-menu-{project}-{ix}")),
                "board-row",
                icons::icon(icons::MENU_DOTS)
                    .size(px(14.))
                    .text_color(theme.text_faint),
                Menu::Board(project, ix),
                cx,
            )
            .children(self.board_menu(project, ix, cx)),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.open_board(project, ix, cx)))
        .into_any_element()
    }

    fn board_menu(&self, project: usize, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Board(project, ix)) {
            return None;
        }
        let rows = vec![
            menu::row(
                Item::action("Rename").with_icon(icons::PEN_NEW_SQUARE),
                move |this, window, cx| {
                    this.start_rename(Renaming::Board { project, ix }, window, cx);
                },
            ),
            menu::row(
                Item::action("Delete").with_icon(icons::TRASH_BIN_MINIMALISTIC),
                move |this, _, cx| this.delete_board(project, ix, cx),
            ),
        ];
        let id = SharedString::from(format!("board-menu-card-{project}-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    fn session_menu(&self, id: u64, archived: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Session(id)) {
            return None;
        }
        let mut rows = vec![menu::row(
            Item::action("Rename").with_icon(icons::PEN_NEW_SQUARE),
            move |this, window, cx| this.start_rename(Renaming::Session(id), window, cx),
        )];
        if !archived {
            rows.push(menu::row(
                Item::action("Archive").with_icon(icons::ARCHIVE_MINIMALISTIC),
                move |this, _, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.archive_session(id, cx));
                },
            ));
        }
        rows.push(menu::row(
            Item::action("Delete").with_icon(icons::TRASH_BIN_MINIMALISTIC),
            move |this, _, cx| this.close_session(id, cx),
        ));
        let id = SharedString::from(format!("session-menu-{id}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// The field, in the row's place. It carries its own press: `TextField`
    /// does not focus itself, and a press that reached the row would open what
    /// is being named out from under the name.
    fn name_field(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.name_field.read(cx).focus_handle(cx), cx);
                }),
            )
            // Pressing anywhere else is finishing, not abandoning — the name
            // typed is the name meant. `escape` is what discards.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.commit_name(&CommitName, window, cx);
            }))
            .child(self.name_field.clone())
            .into_any_element()
    }

    fn start_rename(&mut self, what: Renaming, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let label = match what {
            Renaming::Session(id) => workspace
                .session(id)
                .map(ChatSession::label)
                .unwrap_or_default(),
            Renaming::Board { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.boards.get(ix))
                .map(|board| board.name.clone())
                .unwrap_or_default(),
        };
        self.name_field
            .update(cx, |field, cx| field.set_content(label, cx));
        self.renaming = Some(what);
        window.focus(&self.name_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, _: &CommitName, _: &mut Window, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        let name = self.name_field.read(cx).content().to_string();
        self.workspace.update(cx, |workspace, cx| match what {
            Renaming::Session(id) => workspace.rename_session(id, name, cx),
            Renaming::Board { project, ix } => workspace.rename_board(project, ix, name, cx),
        });
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, _: &DismissName, _: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
