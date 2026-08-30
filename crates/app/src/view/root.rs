//! Root view: the projects rail, and the chat column beside it.

use crate::{
    model::{
        session::{ChatSession, PlanStatus},
        settings::Settings,
        state::State,
        workspace::Workspace,
    },
    utils,
    view::{
        board::{self, Editing},
        composer::{self, Composer, ComposerEvent},
        settings_window::{self, SettingsWindow},
        transcript,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Context, Div, DragMoveEvent, Empty, Entity, FocusHandle,
        Focusable as _, FontWeight, KeyBinding, MouseButton, PathPromptOptions, Render,
        SharedString, Stateful, Task, Window, WindowHandle, actions, div, prelude::*, px, svg,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        icons,
        input::TextField,
        loaders, popover,
        tooltip::Tooltip,
        widgets::{
            ButtonStyle, Buttons, Content, Layout, SPLIT_HANDLE_HIT, Scaffolding, SplitDrag,
            SplitStyle,
        },
    },
};
use cacp::schema::PermissionOptionKind;
use std::time::{Duration, SystemTime};

actions!(
    cydonia,
    [
        NewSession,
        OpenProject,
        OpenSettings,
        CommitName,
        DismissName
    ]
);

/// How often the rail redraws for its relative times. A minute, because that
/// is the finest thing [`utils::ago`] says.
const TICK: Duration = Duration::from_secs(60);

/// Claimed on the rename field so `enter` files the name and `escape` drops it.
const RENAME_CONTEXT: &str = "CydoniaSessionName";

const SIDEBAR_DEFAULT: f32 = 200.;
const SIDEBAR_MIN: f32 = 180.;
const SIDEBAR_MAX: f32 = 420.;

/// Where the traffic lights sit in from the window's left edge — the
/// gallery's rail grid, which the sidebar's own 16pt padding does not share.
const RAIL_PAD: f32 = 20.;

/// How far the content card floats in from the window's edges. The rail runs
/// to the floor behind it, so the frost reads as one shell under the card.
const SHELL_INSET: f32 = 8.;

/// macOS traffic light diameter — AppKit owns the buttons and reports their
/// frame, so nothing here can derive it. Measured on macOS 26.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;

/// Where the traffic lights go, for `TitlebarOptions::traffic_light_position`:
/// the rail's grid across, and down by half the band the rail reserves for
/// them. macOS sizes the button container to `height + 2y`.
pub const TRAFFIC_LIGHT_X: f32 = RAIL_PAD;
pub const TRAFFIC_LIGHT_Y: f32 = (Theme::HEADER_HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.;

/// Between the lights' centres, as AppKit lays them out. Measured on macOS 26.
const TRAFFIC_LIGHT_SPACING: f32 = 23.;

/// Where the head band's own controls start: clear of the three lights AppKit
/// puts down from [`TRAFFIC_LIGHT_X`], plus the rail's gutter. bezel's own
/// inset is for lights left where AppKit wanted them, which these are not.
const RAIL_HEAD_INSET: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_LIGHT_X + 2. * TRAFFIC_LIGHT_SPACING + TRAFFIC_LIGHT_SIZE + 6.
} else {
    8.
};

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewSession, None),
        KeyBinding::new("cmd-o", OpenProject, None),
        // What macOS binds Preferences to in every other app.
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]);
}

/// Which pane the content card shows. A property of the window, not of a
/// project — switching projects must not teleport you to another pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Board,
    Article,
    Table,
}

/// What the rail needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    id: u64,
    label: String,
    icon: Option<SharedString>,
    streaming: bool,
    updated: SystemTime,
    archived: bool,
}

/// Which menu is open. One field rather than a flag each, so opening one
/// closes the rest by construction.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    /// The `+` on a project heading: what to start here.
    Add(usize),
    /// The `···` on a project heading: what to do to the project.
    Project(usize),
    /// The `···` on a session row.
    Session(u64),
}

/// The root view. It owns no app state — only the chrome's own: how wide the
/// rail is, which pane is showing, and whichever card is being written.
pub struct Cydonia {
    pub(crate) workspace: Entity<Workspace>,
    sidebar_open: bool,
    sidebar_width: f32,
    composer: Entity<Composer>,
    settings_window: Option<WindowHandle<SettingsWindow>>,
    pub(crate) pane: Pane,
    pub(crate) editing: Option<Editing>,
    pub(crate) card_field: Entity<TextField>,
    menu: Option<Menu>,
    /// The session whose name is being typed, and the field it is typed in.
    renaming: Option<u64>,
    name_field: Entity<TextField>,
    /// Redraws the rail once a minute so the relative times on it stay true
    /// with nobody touching the window. One timer, not one per row.
    _tick: Task<()>,
}

impl Cydonia {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(Composer::new);
        cx.subscribe(
            &composer,
            |this, _, event: &ComposerEvent, cx| match event {
                ComposerEvent::Submit(text) => this.submit(text.clone(), cx),
                ComposerEvent::Cancel => this.cancel_turn(cx),
                ComposerEvent::Agent(ix) => this.pick_agent(*ix, cx),
            },
        )
        .detach();

        let card_field = board::field(cx);
        let name_field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(RENAME_CONTEXT)
                .with_placeholder("name this session…")
        });
        let tick = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    return;
                }
            }
        });
        let workspace = cx.new(|cx| Workspace::new(settings, state, cx));
        // The model is the only thing that says a session appeared or a turn
        // ended; the composer's placeholder, commands and busy state are all
        // read back from it rather than pushed by whoever caused the change.
        cx.observe(&workspace, |this, _, cx| this.sync_composer(cx))
            .detach();

        let mut this = Self {
            workspace,
            sidebar_open: true,
            sidebar_width: SIDEBAR_DEFAULT,
            composer,
            settings_window: None,
            pane: Pane::Chat,
            editing: None,
            card_field,
            menu: None,
            renaming: None,
            name_field,
            _tick: tick,
        };
        this.sync_composer(cx);
        this
    }

    pub fn composer_focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer.focus_handle(cx)
    }

    /// Run `f` on the active session. Every composer action is this shape:
    /// the view knows which session is in front, the model owns it.
    fn with_active(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut ChatSession)) {
        let Some(id) = self.workspace.read(cx).active_id() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.with_session(id, cx, f));
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        self.with_active(cx, |chat| chat.send(text));
    }

    fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        self.with_active(cx, |chat| chat.cancel());
    }

    /// The composer's agent chip. An ACP session is bound to the process that
    /// serves it, so picking another agent opens a session rather than
    /// swapping one out from under a transcript.
    fn pick_agent(&mut self, ix: usize, cx: &mut Context<Self>) {
        let entry = self.workspace.read(cx).settings.agents.get(ix).cloned();
        if let Some(entry) = entry {
            self.show_pane(Pane::Chat, cx);
            self.workspace
                .update(cx, |workspace, cx| workspace.new_session(entry, None, cx));
        }
    }

    fn new_session_action(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Chat, cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(entry) = workspace.preferred_agent() {
                workspace.new_session(entry, None, cx);
            }
        });
    }

    /// Leaving a project is the moment a half-written card has to be filed:
    /// the spot it points at belongs to the board being navigated away from.
    pub(crate) fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_project(ix, cx));
    }

    pub(crate) fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.close_project(ix, cx));
    }

    pub(crate) fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.show_pane(Pane::Chat, cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_session(id, cx));
    }

    pub(crate) fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.close_session(id, cx));
    }

    fn open_settings_action(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(cx);
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        self.settings_window = settings_window::open(workspace, self.settings_window, cx);
    }

    fn open_project_action(&mut self, _: &OpenProject, _: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.open_project(path, cx));
            });
        })
        .detach();
    }

    /// What the composer needs from the session it is pointed at: the agent's
    /// name, its commands, whether a turn is in flight, and the agents it can
    /// be swapped for.
    fn sync_composer(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let agents: Vec<composer::Agent> = workspace
            .settings
            .agents
            .iter()
            .map(|entry| composer::Agent {
                name: entry.name.clone().into(),
                icon: workspace.agent_icon(&entry.name),
            })
            .collect();
        let chat = workspace.active_session();
        let placeholder = chat.map_or_else(
            || "message the agent…".to_owned(),
            |chat| format!("message {}…", chat.entry.name),
        );
        let commands = chat.map(|chat| chat.commands.clone()).unwrap_or_default();
        let streaming = chat.is_some_and(|chat| chat.streaming);
        let current = chat
            .map(|chat| chat.entry.name.clone())
            .and_then(|name| agents.iter().position(|agent| agent.name == name));
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx);
            composer.set_commands(&commands, cx);
            composer.set_streaming(streaming, cx);
            composer.set_agents(&agents, current, cx);
        });
    }

    // ── chrome ───────────────────────────────────────────────────────

    // ── menus ────────────────────────────────────────────────────────

    fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        cx.notify();
    }

    /// A `···` or `+` that opens `menu`, revealed on the row's hover.
    fn menu_button(
        &self,
        id: impl Into<gpui::ElementId>,
        group: &'static str,
        mark: impl IntoElement,
        menu: Menu,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id.into())
            .flex_none()
            .relative()
            // An open menu keeps its trigger on show — by then the pointer is
            // over the menu, not the row that opened it.
            .when(self.menu != Some(menu), |el| {
                el.invisible().group_hover(group, |el| el.visible())
            })
            .rounded(px(Theme::control_radius()))
            .p(px(3.))
            .cursor_pointer()
            .child(mark)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(menu, cx);
            }))
    }

    /// The card every rail menu hangs in, dismissed by a press outside it.
    fn menu_card(&self, rows: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        popover::popover_card(&theme)
            .w(px(170.))
            .child(div().flex().flex_col().children(rows))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }))
            .into_any_element()
    }

    fn menu_row(
        &self,
        key: impl Into<SharedString>,
        glyph: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let key = key.into();
        popover::menu_row(&theme, false, Fade::new(painter, key.clone()))
            .id(key)
            .child(
                icons::icon(glyph)
                    .size(px(13.))
                    .flex_none()
                    .text_color(theme.text_faint),
            )
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.menu = None;
                act(this, window, cx);
                cx.notify();
            }))
            .into_any_element()
    }

    // ── the rail ─────────────────────────────────────────────────────

    /// One session: its mark and name, with when it last had something to say
    /// under them.
    fn session_row(&self, row: SessionRow, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let id = row.id;
        let selected =
            self.showing(cx) == Pane::Chat && self.workspace.read(cx).active_id() == Some(id);
        let tint = if selected {
            theme.text
        } else {
            theme.text_muted
        };
        // The agent's own mark, in the label's colour rather than any of its
        // own: every icon the registry publishes is a `currentColor` glyph, so
        // tinting is the only colour it will ever have. While a turn is in
        // flight the orb stands in its place — the same one the transcript
        // works under.
        let mark = if row.streaming {
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
            match row.icon {
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
                .text_size(px(13.))
                // What the field pins itself to. Left to gpui's default the
                // label's line box is φ×13, and renaming would resize the row
                // under the name being typed.
                .line_height(px(18.))
                .text_color(tint)
                .child(row.label)
                .into_any_element()
        };

        div()
            .id(("session", id))
            .group("session-row")
            .ml(px(18.))
            .mr(px(8.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(Theme::control_radius()))
            .flex()
            .flex_col()
            .gap(px(1.))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.glass_hover()))
            .hover(|el| el.bg(theme.glass_hover()))
            .child(
                div()
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
                            ui::dots(&theme),
                            Menu::Session(id),
                            cx,
                        )
                        .children(self.session_menu(id, row.archived, cx)),
                    ),
            )
            // The second line is indented past the mark so the two read as one
            // block rather than a list of times.
            .child(
                div()
                    .ml(px(22.))
                    .text_size(px(11.))
                    .text_color(theme.text_faint)
                    .child(match row.archived {
                        true => format!("archived · {}", utils::ago(row.updated)),
                        false => utils::ago(row.updated),
                    }),
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

    // ── renaming ─────────────────────────────────────────────────────

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

    fn commit_name(&mut self, _: &CommitName, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.renaming.take() else {
            return;
        };
        let name = self.name_field.read(cx).content().to_string();
        self.workspace
            .update(cx, |workspace, cx| workspace.rename_session(id, name, cx));
        cx.notify();
    }

    fn dismiss_name(&mut self, _: &DismissName, _: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    /// One project in the rail: a heading that folds, and everything in the
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
                updated: chat.updated,
                archived: chat.archive.is_some(),
            })
            .collect();
        let articles: Vec<(usize, String)> = project
            .articles
            .iter()
            .enumerate()
            .map(|(n, article)| (n, article.title()))
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
                            .text_size(px(11.))
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
                            ui::dots(&theme),
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

    /// What the `+` starts here. Session first: it is what the rail is for.
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

    /// The band the traffic lights float in. It belongs to whichever column
    /// runs along the window's left edge — the rail while it is open, the
    /// content column once it is not — so the toggle keeps its place across
    /// the collapse.
    fn rail_head(&self, window: &Window, cx: &mut Context<Self>) -> Div {
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
                RAIL_HEAD_INSET
            }))
            .pr(px(8.))
            .flex()
            .flex_row()
            .items_center()
            .child(
                ui::ghost(&theme, "toggle-sidebar")
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

    fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            // The head band carries the one action that is not about a
            // project you already have, out at the rail's trailing edge.
            .child(
                self.rail_head(window, cx).child(div().flex_1()).child(
                    ui::ghost(&theme, "open-project")
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
                ui::ghost(&theme, "settings")
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
                            .text_size(px(13.))
                            .text_color(theme.text_muted)
                            .child("Settings"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
            )
    }

    /// The session in front, or the invitation to open one.
    fn conversation(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.workspace.read(cx).active_id() {
            Some(id) => self
                .workspace
                .update(cx, |workspace, cx| match workspace.session(id) {
                    Some(chat) => transcript::render(chat, window, cx),
                    None => div().flex_1().into_any_element(),
                }),
            None => Theme::of(cx)
                .empty_state(
                    icons::CHAT_ROUND_LINE,
                    "No session",
                    "⌘N to start one in this project.",
                )
                .flex_1()
                .into_any_element(),
        }
    }

    /// Which pane is on screen, as against [`Self::pane`], which is the one
    /// asked for. They part when the article it points at is gone — deleted,
    /// or in a project that has none open — and the conversation stands in.
    /// The rail reads this, not the request: a row lit for a pane nobody can
    /// see is the second selection the eye finds.
    pub(crate) fn showing(&self, cx: &App) -> Pane {
        match self.pane {
            Pane::Article if self.workspace.read(cx).active_article().is_none() => Pane::Chat,
            Pane::Table if self.workspace.read(cx).active_table().is_none() => Pane::Chat,
            pane => pane,
        }
    }

    fn chat(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let open = self.workspace.read(cx).active_project().is_some();
        // Nothing to send to: archiving closed the connection, so the composer
        // stack would be a prompt box wired to a dead process.
        let live = !self
            .workspace
            .read(cx)
            .active_session()
            .is_some_and(|chat| chat.archive.is_some());
        let body = if !open {
            self.no_project(cx)
        } else {
            match self.showing(cx) {
                Pane::Chat => self.conversation(window, cx),
                Pane::Board => self.board(cx),
                Pane::Article => match self.article(cx) {
                    Some(article) => article,
                    None => self.conversation(window, cx),
                },
                Pane::Table => match self.table(cx) {
                    Some(table) => table,
                    None => self.conversation(window, cx),
                },
            }
        };

        let card = div()
            .flex_1()
            .min_h_0()
            // With the rail gone the head band above it already clears the
            // traffic lights, and a margin on top of that doubles the air.
            .mt(px(if self.sidebar_open { SHELL_INSET } else { 0. }))
            .ml(px(SHELL_INSET))
            .mr(px(SHELL_INSET))
            .flex()
            .flex_col()
            .rounded(px(Theme::panel_radius()))
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(body)
            .when(open && live, |card| {
                card.child(
                    div().flex_none().flex().justify_center().child(
                        div()
                            .w_full()
                            .max_w(px(720.))
                            .px(px(24.))
                            .pb(px(20.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(self.plan(cx))
                            .children(self.permission(cx))
                            .children(self.queue(cx))
                            .child(self.composer.clone()),
                    ),
                )
            });

        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .when(!self.sidebar_open, |column| {
                column.child(self.rail_head(window, cx))
            })
            .child(card)
            .when(open, |column| column.child(self.pane_switch(cx)))
    }

    /// The shell strip under the content card: on the frost, not on the card.
    /// What it holds is about the pane you are in rather than anything inside
    /// it, so it sits outside the surface it switches.
    ///
    /// One button, labelled with where it goes — with two panes, a segmented
    /// track spends a permanent slot restating the one you are already
    /// looking at.
    fn pane_switch(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let (glyph, label, to) = match self.pane {
            Pane::Board => (icons::CHAT_ROUND_LINE, "Chat", Pane::Chat),
            Pane::Chat | Pane::Article | Pane::Table => (icons::LIST, "Board", Pane::Board),
        };
        div()
            .flex_none()
            .py(px(SHELL_INSET))
            .px(px(SHELL_INSET + 6.))
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .child(
                ui::ghost(&theme, "pane-switch")
                    .px(px(8.))
                    .py(px(4.))
                    .gap(px(6.))
                    .child(
                        icons::icon(glyph)
                            .size(px(13.))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.show_pane(to, cx))),
            )
    }

    /// Nothing is open, so there is nowhere to send a prompt — the only thing
    /// on offer is a folder.
    fn no_project(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        theme
            .empty_state(
                icons::FOLDER,
                "No project open",
                "An agent runs in a directory. Pick one to start.",
            )
            .flex_1()
            .child(
                theme
                    .button(
                        "Open folder…",
                        ButtonStyle::Prominent,
                        Some(Fade::new(painter, "open-project-empty")),
                    )
                    .id("open-project-empty")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_project_action(&OpenProject, window, cx);
                    })),
            )
            .into_any_element()
    }

    /// The agent's plan, while it still has something left to do.
    fn plan(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        if chat.plan.is_empty() || chat.plan.iter().all(|(_, s)| *s == PlanStatus::Done) {
            return None;
        }
        Some(
            theme
                .group_box()
                .mt(px(0.))
                .px(px(12.))
                .py(px(8.))
                .gap(px(4.))
                .text_size(px(12.))
                .children(chat.plan.iter().map(|(text, status)| {
                    let (icon, tone) = match status {
                        PlanStatus::Done => (icons::CHECK, theme.success),
                        PlanStatus::Active => (icons::ALT_ARROW_RIGHT, theme.accent),
                        PlanStatus::Pending => (icons::CHECKLIST, theme.text_faint),
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap(px(8.))
                        .text_color(theme.text_muted)
                        .child(icons::icon(icon).size(px(12.)).text_color(tone))
                        .child(text.clone())
                })),
        )
    }

    /// The agent's tool-authorization request, one button per option.
    fn permission(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        let prompt = chat.permission.as_ref()?;
        let id = chat.id;
        let painter = Painter::of(cx);
        Some(
            theme
                .group_box()
                .mt(px(0.))
                .border_color(theme.accent)
                .px(px(12.))
                .py(px(10.))
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(prompt.title.clone()),
                )
                .child(div().flex().flex_row().gap(px(8.)).children(
                    prompt.options.iter().enumerate().map(|(ix, option)| {
                        let style = match option.kind {
                            PermissionOptionKind::AllowOnce => ButtonStyle::Prominent,
                            PermissionOptionKind::RejectOnce
                            | PermissionOptionKind::RejectAlways => ButtonStyle::Destructive,
                            _ => ButtonStyle::Ghost,
                        };
                        let fade = Fade::new(painter, format!("permission-{id}-{ix}"));
                        let option_id = option.id.clone();
                        theme
                            .button(option.name.clone(), style, Some(fade))
                            .id(("permission", ix))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let option_id = option_id.clone();
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.with_session(id, cx, |chat| {
                                        chat.respond_permission(option_id);
                                    });
                                });
                            }))
                    }),
                )),
        )
    }

    /// Prompts waiting for the in-flight turn.
    fn queue(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        if chat.queue.is_empty() {
            return None;
        }
        Some(
            div().flex().flex_row().flex_wrap().gap(px(6.)).children(
                chat.queue
                    .iter()
                    .map(|text| theme.badge(format!("queued · {text}"))),
            ),
        )
    }
}

impl Render for Cydonia {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(theme.window_bg())
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_size(px(14.))
            .on_action(cx.listener(Self::new_session_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::open_settings_action))
            .on_action(cx.listener(Self::commit_name))
            .on_action(cx.listener(Self::dismiss_name))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width =
                        f32::from(event.event.position.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                    cx.notify();
                }),
            )
            .when(self.sidebar_open, |root| {
                root.child(self.sidebar(window, cx))
            })
            .child(self.chat(window, cx))
            // Rides in the gap between the rail and the card rather than
            // sitting in flow, so neither pane has to give up a column.
            .when(self.sidebar_open, |root| {
                root.child(
                    theme
                        .split_handle(Axis::Horizontal, SplitStyle::Ghost)
                        .id("sidebar-split")
                        .absolute()
                        .top_0()
                        .left(px(self.sidebar_width - SPLIT_HANDLE_HIT / 2.))
                        .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
                )
            })
    }
}
