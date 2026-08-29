//! Root view: the projects rail, and the chat column beside it.

use crate::{
    agents,
    board::{self, Editing},
    composer::{self, Composer, ComposerEvent},
    project::Project,
    session::{ChatSession, PlanStatus},
    settings::{self, Settings},
    settings_window::{self, SettingsWindow},
    state::{self, State},
};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable as _,
        FontWeight, KeyBinding, PathPromptOptions, Render, SharedString, Window, WindowHandle, div,
        prelude::*, px, svg,
    },
    motion::{Fade, Painter},
    theme::{Theme, appearance::AppearanceMode},
    ui::{
        icons,
        input::TextField,
        tooltip::Tooltip,
        widgets::{
            self, ButtonStyle, Buttons, Content, Layout, SPLIT_HANDLE_HIT, Scaffolding, SplitDrag,
            SplitStyle,
        },
    },
};
use cacp::schema::PermissionOptionKind;
use gpui::actions;
use std::{collections::HashMap, path::PathBuf};

actions!(cydonia, [NewSession, OpenProject, OpenSettings]);

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

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewSession, None),
        KeyBinding::new("cmd-o", OpenProject, None),
        // What macOS binds Preferences to in every other app.
        KeyBinding::new("cmd-,", OpenSettings, None),
    ]);
}

pub struct Cydonia {
    pub settings: Settings,
    pub(crate) projects: Vec<Project>,
    pub(crate) active: Option<usize>,
    next_id: u64,
    sidebar_width: f32,
    composer: Entity<Composer>,
    appearance: AppearanceMode,
    settings_window: Option<WindowHandle<SettingsWindow>>,
    /// Which pane the content card shows. A property of the window, not of a
    /// project — switching projects must not teleport you to the other pane.
    pub(crate) board_open: bool,
    pub(crate) editing: Option<Editing>,
    pub(crate) card_field: Entity<TextField>,
    /// The registry's mark for each configured agent, by name. Empty until the
    /// catalog lands, and stays empty offline.
    agent_icons: HashMap<String, SharedString>,
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
        let projects: Vec<Project> = state.projects.into_iter().map(Project::new).collect();
        let active = (!projects.is_empty()).then_some(state.active);
        let mut this = Self {
            settings,
            projects,
            active,
            next_id: 0,
            sidebar_width: SIDEBAR_DEFAULT,
            composer,
            appearance: state.appearance,
            settings_window: None,
            board_open: false,
            editing: None,
            card_field,
            agent_icons: HashMap::new(),
        };
        this.open_first_session(cx);
        this.sync_composer(cx);
        this.load_agent_icons(cx);
        this
    }

    /// Fetch the catalog and keep each configured agent's icon. Off the UI
    /// thread — the registry is a blocking fetch on a cold cache — and a
    /// failure just leaves the map empty.
    fn load_agent_icons(&mut self, cx: &mut Context<Self>) {
        let configured = self.settings.agents.clone();
        cx.spawn(async move |this, cx| {
            let icons = cx
                .background_executor()
                .spawn(async move { agents::icons(&configured) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.agent_icons = icons;
                app.sync_composer(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The registry's mark for whatever this session runs on.
    fn agent_icon(&self, name: &str) -> Option<SharedString> {
        self.agent_icons.get(name).cloned()
    }

    pub fn composer_focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer.focus_handle(cx)
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(id) = self.active_id() {
            self.with_session(id, cx, |chat| chat.send(text));
        }
    }

    fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.active_id() {
            self.with_session(id, cx, |chat| chat.cancel());
        }
    }

    /// The composer's agent chip. An ACP session is bound to the process that
    /// serves it, so picking another agent opens a session rather than
    /// swapping one out from under a transcript.
    fn pick_agent(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.settings.agents.get(ix).cloned() {
            self.new_session(entry, None, cx);
        }
    }

    fn new_session_action(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.preferred_agent() {
            self.new_session(entry, None, cx);
        }
    }

    /// Which agent an unasked-for session runs on: whoever the project is
    /// already talking to, else the first one configured.
    pub(crate) fn preferred_agent(&self) -> Option<settings::Agent> {
        self.active_session()
            .map(|chat| chat.entry.clone())
            .or_else(|| self.settings.agents.first().cloned())
    }

    /// The settings window's choice. bezel repaints on `set_mode`; the state
    /// file is what makes it survive a relaunch.
    pub fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.appearance = mode;
        bezel::theme::appearance::set_mode(mode, cx);
        state::save(&self.projects, self.active, mode);
        cx.notify();
    }

    fn open_settings_action(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(cx);
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let app = cx.entity();
        self.settings_window = settings_window::open(app, self.settings_window, cx);
    }

    // ── projects ─────────────────────────────────────────────────────

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
            let _ = this.update(cx, |app, cx| app.open_project(path, cx));
        })
        .detach();
    }

    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let open = self.projects.iter().position(|p| p.path == path);
        let ix = match open {
            Some(ix) => ix,
            None => {
                self.projects.push(Project::new(path));
                self.projects.len() - 1
            }
        };
        self.select_project(ix, cx);
    }

    pub fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.commit_edit(cx);
        self.active = Some(ix);
        self.open_first_session(cx);
        self.sync_composer(cx);
        state::save(&self.projects, self.active, self.appearance);
        cx.notify();
    }

    /// Drop the project: its sessions go with it, and each session's shutdown
    /// sender goes with that — the agent processes die here.
    pub fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.commit_edit(cx);
        self.projects.remove(ix);
        self.active = self.active.and_then(|active| {
            let next = if active > ix { active - 1 } else { active };
            (!self.projects.is_empty()).then(|| next.min(self.projects.len() - 1))
        });
        self.open_first_session(cx);
        self.sync_composer(cx);
        state::save(&self.projects, self.active, self.appearance);
        cx.notify();
    }

    /// A project talks to an agent the moment it is looked at: the tab in
    /// front opens its first session, and the tabs behind it spawn nothing.
    fn open_first_session(&mut self, cx: &mut Context<Self>) {
        if self
            .active_project()
            .is_none_or(|project| !project.sessions.is_empty())
        {
            return;
        }
        if let Some(entry) = self.settings.agents.first().cloned() {
            self.new_session(entry, None, cx);
        }
    }

    pub(crate) fn active_project(&self) -> Option<&Project> {
        self.active.and_then(|ix| self.projects.get(ix))
    }

    // ── sessions ─────────────────────────────────────────────────────

    /// Open a session in the active project. `seed` is its first prompt, sent
    /// as soon as the agent is up — what a dispatched card rides in on.
    pub fn new_session(
        &mut self,
        entry: settings::Agent,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let ix = self.active?;
        let id = self.next_id;
        self.next_id += 1;
        let chat = ChatSession::connect(id, entry, self.projects[ix].path.clone(), seed, cx);
        let project = &mut self.projects[ix];
        project.sessions.push(chat);
        project.active = Some(id);
        self.sync_composer(cx);
        cx.notify();
        Some(id)
    }

    /// Every project's sessions are on show, so picking one brings its project
    /// forward with it.
    pub fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(ix) = self.project_of(id) else {
            return;
        };
        self.projects[ix].active = Some(id);
        if self.active != Some(ix) {
            self.active = Some(ix);
            state::save(&self.projects, self.active, self.appearance);
        }
        self.sync_composer(cx);
        cx.notify();
    }

    fn project_of(&self, id: u64) -> Option<usize> {
        self.projects
            .iter()
            .position(|project| project.session(id).is_some())
    }

    /// Drop the session: the shutdown sender goes with it and the agent
    /// process dies.
    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.project_of(id).map(|ix| &mut self.projects[ix]) else {
            return;
        };
        project.sessions.retain(|chat| chat.id != id);
        if project.active == Some(id) {
            project.active = project.sessions.last().map(|chat| chat.id);
            self.sync_composer(cx);
        }
        cx.notify();
    }

    /// Any session, in whichever project holds it — the pump that feeds a
    /// session knows only its id, and must not care which tab it sits behind.
    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.projects.iter().find_map(|project| project.session(id))
    }

    /// Run `f` on the session (when it still exists) and repaint.
    pub fn with_session(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ChatSession),
    ) {
        let found = self
            .projects
            .iter_mut()
            .find_map(|project| project.session_mut(id));
        if let Some(chat) = found {
            f(chat);
            if self.active_id() == Some(id) {
                self.sync_composer(cx);
            }
            cx.notify();
        }
    }

    /// What the composer needs from the session it is pointed at: the agent's
    /// name, its commands, whether a turn is in flight, and the agents it can
    /// be swapped for.
    fn sync_composer(&mut self, cx: &mut Context<Self>) {
        let agents: Vec<composer::Agent> = self
            .settings
            .agents
            .iter()
            .map(|entry| composer::Agent {
                name: entry.name.clone().into(),
                icon: self.agent_icon(&entry.name),
            })
            .collect();
        let chat = self.active_session();
        let placeholder = chat.map_or_else(
            || "message the agent…".to_owned(),
            |chat| format!("message {}…", chat.entry.name),
        );
        let commands = chat.map(|chat| chat.commands.clone()).unwrap_or_default();
        let streaming = chat.is_some_and(|chat| chat.streaming);
        let current = chat.and_then(|chat| {
            agents
                .iter()
                .position(|agent| agent.name == chat.entry.name)
        });
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx);
            composer.set_commands(&commands, cx);
            composer.set_streaming(streaming, cx);
            composer.set_agents(&agents, current, cx);
        });
    }

    /// The session opened. Temporary dev hook: `CYDONIA_TEST_PROMPT` sends
    /// a prompt right away so streaming can be verified without a composer.
    pub fn session_connected(&mut self, id: u64, cx: &mut Context<Self>) {
        self.with_session(id, cx, |chat| {
            if let Some(seed) = chat.seed.take() {
                chat.send(seed);
            }
        });
        if let Ok(prompt) = std::env::var("CYDONIA_TEST_PROMPT") {
            self.with_session(id, cx, |chat| chat.send(prompt));
        }
        cx.notify();
    }

    fn active_session(&self) -> Option<&ChatSession> {
        self.active_project().and_then(Project::active_session)
    }

    fn active_id(&self) -> Option<u64> {
        self.active_project().and_then(|project| project.active)
    }

    // ── chrome ───────────────────────────────────────────────────────

    fn session_row(&self, chat: &ChatSession, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let id = chat.id;
        let selected = self.active_id() == Some(id);
        let label = if chat.title.is_empty() {
            chat.entry.name.clone()
        } else {
            chat.title.clone()
        };
        let tone = if chat.lost {
            theme.danger
        } else if chat.streaming {
            theme.accent
        } else if chat.session.is_some() {
            theme.success
        } else {
            theme.text_faint
        };

        div()
            .id(("session", id))
            .group("session-row")
            .ml(px(18.))
            .mr(px(8.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(Theme::control_radius()))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.glass_hover()))
            .hover(|el| el.bg(theme.glass_hover()))
            // The agent's own mark where the catalog has one. The dot stays
            // the answer for an agent the registry doesn't publish — a local
            // binary, or a first run with no catalog yet.
            //
            // The mark takes the label's colour, not the session's: every icon
            // the registry publishes is a `currentColor` glyph, so tinting is
            // the only colour it will ever have, and reading it as status
            // would make the agent's identity change with its state.
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match self.agent_icon(&chat.entry.name) {
                        Some(path) => svg()
                            .path(path)
                            .size(px(14.))
                            .flex_none()
                            .text_color(if selected {
                                theme.text
                            } else {
                                theme.text_muted
                            })
                            .into_any_element(),
                        None => widgets::status_dot(tone).into_any_element(),
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .child(label),
            )
            .child(
                div()
                    .id(("close", id))
                    .flex_none()
                    .invisible()
                    .group_hover("session-row", |el| el.visible())
                    .rounded(px(Theme::control_radius()))
                    .p(px(2.))
                    .child(
                        icons::icon(icons::CLOSE)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_session(id, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(id, cx);
            }))
    }

    /// One project in the rail: a heading that selects it, with its sessions
    /// under it. Every project shows its own, so the rail is the whole map.
    fn project_section(
        &self,
        ix: usize,
        project: &Project,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let active = self.active == Some(ix);
        let path = project.path.display().to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .id(("project", ix))
                    .group("project-head")
                    .mx(px(8.))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(Theme::control_radius()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.glass_hover()))
                    .tooltip(move |window, cx| Tooltip::text(path.clone(), window, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if active { theme.text } else { theme.text_faint })
                            .child(project.name()),
                    )
                    .child(
                        div()
                            .id(("new-session", ix))
                            .flex_none()
                            .invisible()
                            .group_hover("project-head", |el| el.visible())
                            .rounded(px(Theme::control_radius()))
                            .p(px(2.))
                            .child(
                                icons::icon(icons::PEN_NEW_SQUARE)
                                    .size(px(12.))
                                    .text_color(theme.text_faint),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.select_project(ix, cx);
                                this.new_session_action(&NewSession, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id(("close-project", ix))
                            .flex_none()
                            .invisible()
                            .group_hover("project-head", |el| el.visible())
                            .rounded(px(Theme::control_radius()))
                            .p(px(2.))
                            .child(
                                icons::icon(icons::CLOSE)
                                    .size(px(11.))
                                    .text_color(theme.text_faint),
                            )
                            // Without this the heading's own click runs next
                            // and selects whichever project slid into the
                            // closed one's index.
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_project(ix, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_project(ix, cx);
                    })),
            )
            .children(
                project
                    .sessions
                    .iter()
                    .map(|chat| self.session_row(chat, cx)),
            )
    }

    fn sidebar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
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
            // The traffic lights float over the rail now that no header strip
            // holds them; the band they sit in carries the one action that is
            // not about a project you already have.
            .child(
                div()
                    .flex_none()
                    .h(px(Theme::HEADER_HEIGHT))
                    .pr(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .child(
                        div()
                            .id("open-project")
                            .p(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.glass_hover()))
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
                    .children(
                        self.projects
                            .iter()
                            .enumerate()
                            .map(|(ix, project)| self.project_section(ix, project, cx)),
                    ),
            )
            .child(
                div()
                    .id("settings")
                    .flex_none()
                    .mx(px(8.))
                    .mb(px(8.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(Theme::control_radius()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.glass_hover()))
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

    fn chat(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let open = self.active_project().is_some();
        let body = if !open {
            self.no_project(cx)
        } else if self.board_open {
            self.board(cx)
        } else {
            match self.active_session() {
                Some(chat) => self.transcript(chat, window, cx),
                None => theme
                    .empty_state(
                        icons::CHAT_ROUND_LINE,
                        "No session",
                        "⌘N to start one in this project.",
                    )
                    .flex_1()
                    .into_any_element(),
            }
        };

        let card = div()
            .flex_1()
            .min_h_0()
            .mt(px(SHELL_INSET))
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
            .when(open, |card| {
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
        let board = self.board_open;
        let (glyph, label) = if board {
            (icons::CHAT_ROUND_LINE, "Chat")
        } else {
            (icons::LIST, "Board")
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
                div()
                    .id("pane-switch")
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(Theme::control_radius()))
                    .cursor_pointer()
                    .hover(|el| el.bg(theme.glass_hover()))
                    .flex()
                    .flex_row()
                    .items_center()
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
                    .on_click(cx.listener(move |this, _, _, cx| this.show_board(!board, cx))),
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
        let chat = self.active_session()?;
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
        let chat = self.active_session()?;
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
                                this.with_session(id, cx, |chat| {
                                    chat.respond_permission(option_id);
                                });
                            }))
                    }),
                )),
        )
    }

    /// Prompts waiting for the in-flight turn.
    fn queue(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.active_session()?;
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
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width =
                        f32::from(event.event.position.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                    cx.notify();
                }),
            )
            .child(self.sidebar(cx))
            .child(self.chat(window, cx))
            // Rides in the gap between the rail and the card rather than
            // sitting in flow, so neither pane has to give up a column.
            .child(
                theme
                    .split_handle(Axis::Horizontal, SplitStyle::Ghost)
                    .id("sidebar-split")
                    .absolute()
                    .top_0()
                    .left(px(self.sidebar_width - SPLIT_HANDLE_HIT / 2.))
                    .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
            )
    }
}
