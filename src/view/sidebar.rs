//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it. The window's grid lives in [`crate::view::root`];
//! this draws on it.

use crate::{
    model::session::ChatSession,
    view::{
        component::menu::Menu,
        root::{self, CommitName, Cydonia, DismissName, NewSession, OpenProject, Pane},
    },
};
use bezel::{
    gpui::{
        self, AnyElement, Context, Div, Empty, Focusable as _, FontWeight, MouseButton,
        SharedString, Stateful, Window, div, prelude::*, px, svg,
    },
    motion::Painter,
    theme::Theme,
    ui::{
        icons, loaders, popover,
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
        .ml(px(root::ROW_INDENT))
        .mr(px(root::SIDEBAR_GUTTER))
        .px(px(root::SIDEBAR_GUTTER))
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
        .when(selected, |el| el.bg(theme.glass_hover()))
        .hover(|el| el.bg(theme.glass_hover()))
}

impl Cydonia {
    pub(crate) fn sidebar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let count = self.workspace.read(cx).projects.len();
        let sections: Vec<AnyElement> = (0..count).map(|ix| self.project_section(ix, cx)).collect();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            // No fill of its own: the root already paints the frost, and a
            // second coat of the same tint reads darker than the shell it
            // is supposed to be part of.
            //
            .flex()
            .flex_col()
            // The toolbar carries the one action that is not about a
            // project you already have, out at the sidebar's trailing edge.
            .child(
                self.toolbar(window, cx).child(div().flex_1()).child(
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
                ),
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
                            .text_size(px(root::SIDEBAR_TEXT))
                            .text_color(theme.text_muted)
                            .child("Settings"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
            )
    }

    /// The band the traffic lights float in. It belongs to whichever column
    /// runs along the window's left edge — the sidebar while it is open, the
    /// content column once it is not — so the toggle keeps its place across
    /// the collapse.
    pub(crate) fn toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::of(cx).clone();
        let label = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        div()
            .flex_none()
            .h(px(Theme::HEADER_HEIGHT))
            // Full screen takes the lights away, and the room they needed
            // would be left as a hole.
            .pl(px(if window.is_fullscreen() {
                8.
            } else {
                root::TOOLBAR_INSET
            }))
            .pr(px(8.))
            .flex()
            .flex_row()
            .items_center()
            .child(
                theme
                    .ghost("toggle-sidebar")
                    .p(px(4.))
                    .tooltip(move |window, cx| Tooltip::text(label, window, cx))
                    .child(
                        icons::icon(icons::SIDEBAR_MINIMALISTIC_LEFT)
                            .size(px(14.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
            )
    }

    /// One project in the sidebar: a heading that folds, and everything in the
    /// project under it.
    fn project_section(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(project) = workspace.projects.get(ix) else {
            return Empty.into_any_element();
        };
        let active = workspace.active == Some(ix);
        let expanded = project.expanded;
        let name = project.name();
        let sessions: Vec<SessionRow> = project
            .ordered()
            .map(|chat| SessionRow {
                id: chat.id,
                label: chat.label(),
                icon: workspace.agent_icon(&chat.entry.name),
                streaming: chat.streaming,
                archived: chat.archive.is_some(),
            })
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
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.))
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.glass_hover()))
                    .child(theme.disclosure(expanded))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(root::SIDEBAR_HEADING))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if active { theme.text } else { theme.text_faint })
                            .child(name),
                    )
                    .child(
                        self.menu_button(
                            ("project-add", ix),
                            "project-head",
                            icons::icon(icons::PLUS)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                            Menu::Add(ix),
                            cx,
                        )
                        .children(self.add_menu(ix, cx)),
                    )
                    .child(
                        self.menu_button(
                            ("project-more", ix),
                            "project-head",
                            icons::icon(icons::MENU_DOTS)
                                .size(px(14.))
                                .text_color(theme.text_faint),
                            Menu::Project(ix),
                            cx,
                        )
                        .children(self.project_menu(ix, cx)),
                    )
                    // The heading is the fold. Selecting the project is what
                    // opening something inside it already does.
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_project(ix, cx))),
            )
            .when(expanded, |section| {
                section
                    .children(sessions.into_iter().map(|row| self.session_row(row, cx)))
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
            self.menu_row(
                format!("add-session-{ix}"),
                icons::CHAT_ROUND_LINE,
                "New session",
                cx,
                move |this, window, cx| {
                    this.select_project(ix, cx);
                    this.new_session_action(&NewSession, window, cx);
                },
            ),
            self.menu_row(
                format!("add-article-{ix}"),
                icons::DOCUMENT_ADD,
                "New article",
                cx,
                move |this, window, cx| this.new_article(ix, window, cx),
            ),
            self.menu_row(
                format!("add-table-{ix}"),
                icons::WIDGET,
                "New table",
                cx,
                move |this, _, cx| this.new_table(ix, cx),
            ),
        ];
        Some(popover::anchored_menu_below(
            SharedString::from(format!("add-menu-{ix}")),
            self.menu_card(rows, cx),
            None,
        ))
    }

    /// What the `···` does to the project. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Project(ix)) {
            return None;
        }
        let rows = vec![self.menu_row(
            format!("remove-project-{ix}"),
            icons::TRASH_BIN_MINIMALISTIC,
            "Remove project",
            cx,
            move |this, _, cx| this.close_project(ix, cx),
        )];
        Some(popover::anchored_menu_below(
            SharedString::from(format!("project-menu-{ix}")),
            self.menu_card(rows, cx),
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

        let naming = self.renaming == Some(id);
        let label: AnyElement = if naming {
            // The field carries its own press: `TextField` does not focus
            // itself, and a press that reached the row would select the
            // session out from under the name being typed.
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
                // Pressing anywhere else is finishing, not abandoning — the
                // name typed is the name meant. `escape` is what discards.
                .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    this.commit_name(&CommitName, window, cx);
                }))
                .child(self.name_field.clone())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(root::SIDEBAR_TEXT))
                // What the field pins itself to. Left to gpui's default the
                // label's line box is φ×13, and renaming would resize the row
                // under the name being typed.
                .line_height(px(18.))
                .text_color(tint)
                .child(session.label)
                .into_any_element()
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

    fn session_menu(&self, id: u64, archived: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Session(id)) {
            return None;
        }
        let mut rows = vec![self.menu_row(
            format!("rename-{id}"),
            icons::PEN_NEW_SQUARE,
            "Rename",
            cx,
            move |this, window, cx| this.start_rename(id, window, cx),
        )];
        if !archived {
            rows.push(self.menu_row(
                format!("archive-{id}"),
                icons::ARCHIVE_MINIMALISTIC,
                "Archive",
                cx,
                move |this, _, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.archive_session(id, cx));
                },
            ));
        }
        rows.push(self.menu_row(
            format!("close-{id}"),
            icons::TRASH_BIN_MINIMALISTIC,
            // An archived session has a file behind it, and closing it takes
            // that with the row.
            if archived { "Delete" } else { "Close" },
            cx,
            move |this, _, cx| this.close_session(id, cx),
        ));
        Some(popover::anchored_menu_below(
            SharedString::from(format!("session-menu-{id}")),
            self.menu_card(rows, cx),
            None,
        ))
    }

    fn start_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let label = self
            .workspace
            .read(cx)
            .session(id)
            .map(ChatSession::label)
            .unwrap_or_default();
        self.name_field
            .update(cx, |field, cx| field.set_content(label, cx));
        self.renaming = Some(id);
        window.focus(&self.name_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, _: &CommitName, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.renaming.take() else {
            return;
        };
        let name = self.name_field.read(cx).content().to_string();
        self.workspace
            .update(cx, |workspace, cx| workspace.rename_session(id, name, cx));
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, _: &DismissName, _: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
