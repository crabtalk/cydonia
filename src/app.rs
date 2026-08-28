//! Root view: the project tabs, the sessions rail, and the chat column.

use crate::{
    composer::{Composer, ComposerEvent},
    project::{self, Project},
    session::{ChatSession, PlanStatus},
    settings::{self, Settings},
};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable as _,
        FontWeight, KeyBinding, PathPromptOptions, Render, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{
            self, ButtonStyle, Buttons, Content, Layout, SPLIT_HANDLE_HIT, Scaffolding, SplitDrag,
            SplitStyle,
        },
    },
};
use cacp::schema::PermissionOptionKind;
use gpui::actions;
use std::path::PathBuf;

actions!(cydonia, [NewSession, OpenProject]);

const SIDEBAR_DEFAULT: f32 = 200.;
const SIDEBAR_MIN: f32 = 180.;
const SIDEBAR_MAX: f32 = 420.;

/// Where the traffic lights sit in from the window's left edge — the
/// gallery's rail grid, which the sidebar's own 16pt padding does not share.
const RAIL_PAD: f32 = 20.;

/// Padding inside the content card. Read with the sidebar width it gives the
/// nav its offset, so the tabs sit over the transcript rather than the edge.
const CARD_PAD: f32 = 24.;

/// Padding inside one nav item, subtracted back out of the strip so the tabs
/// start on the card's grid rather than their hit box.
const NAV_ITEM_PAD: f32 = 4.;

/// Clearance under the tab row. `Layout::tab` hangs its active underline 2px
/// below the tab, and the content card is a later sibling that would paint
/// over anything reaching past the nav's own height.
const TAB_UNDERLINE_CLEARANCE: f32 = 3.;

/// macOS traffic light diameter — AppKit owns the buttons and reports their
/// frame, so nothing here can derive it. Measured on macOS 26.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;

/// Where the traffic lights go, for `TitlebarOptions::traffic_light_position`:
/// the sidebar's grid across, the nav strip's centre down. macOS sizes the
/// button container to `height + 2y`, so this `y` is what makes it the strip.
pub const TRAFFIC_LIGHT_X: f32 = RAIL_PAD;
pub const TRAFFIC_LIGHT_Y: f32 = (Theme::HEADER_HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewSession, None),
        KeyBinding::new("cmd-o", OpenProject, None),
    ]);
}

pub struct Cydonia {
    pub settings: Settings,
    projects: Vec<Project>,
    active: Option<usize>,
    next_id: u64,
    sidebar_width: f32,
    composer: Entity<Composer>,
}

impl Cydonia {
    pub fn new(settings: Settings, cx: &mut Context<Self>) -> Self {
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

        let state = project::restore();
        let projects: Vec<Project> = state.projects.into_iter().map(Project::new).collect();
        let active = (!projects.is_empty()).then_some(state.active);
        let mut this = Self {
            settings,
            projects,
            active,
            next_id: 0,
            sidebar_width: SIDEBAR_DEFAULT,
            composer,
        };
        this.open_first_session(cx);
        this.sync_composer(cx);
        this
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
            self.new_session(entry, cx);
        }
    }

    fn new_session_action(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        let entry = self
            .active_session()
            .map(|chat| chat.entry.clone())
            .or_else(|| self.settings.agents.first().cloned());
        if let Some(entry) = entry {
            self.new_session(entry, cx);
        }
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
        self.active = Some(ix);
        self.open_first_session(cx);
        self.sync_composer(cx);
        project::save(&self.projects, self.active);
        cx.notify();
    }

    /// Drop the project: its sessions go with it, and each session's shutdown
    /// sender goes with that — the agent processes die here.
    pub fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.projects.remove(ix);
        self.active = self.active.and_then(|active| {
            let next = if active > ix { active - 1 } else { active };
            (!self.projects.is_empty()).then(|| next.min(self.projects.len() - 1))
        });
        self.open_first_session(cx);
        self.sync_composer(cx);
        project::save(&self.projects, self.active);
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
            self.new_session(entry, cx);
        }
    }

    fn active_project(&self) -> Option<&Project> {
        self.active.and_then(|ix| self.projects.get(ix))
    }

    // ── sessions ─────────────────────────────────────────────────────

    pub fn new_session(&mut self, entry: settings::Agent, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        let id = self.next_id;
        self.next_id += 1;
        let chat = ChatSession::connect(id, entry, self.projects[ix].path.clone(), cx);
        let project = &mut self.projects[ix];
        project.sessions.push(chat);
        project.active = Some(id);
        self.sync_composer(cx);
        cx.notify();
    }

    pub fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.active.map(|ix| &mut self.projects[ix]) else {
            return;
        };
        if project.session(id).is_none() || project.active == Some(id) {
            return;
        }
        project.active = Some(id);
        self.sync_composer(cx);
        cx.notify();
    }

    /// Drop the session: the shutdown sender goes with it and the agent
    /// process dies.
    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(project) = self.active.map(|ix| &mut self.projects[ix]) else {
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
        let agents: Vec<String> = self
            .settings
            .agents
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        let chat = self.active_session();
        let placeholder = chat.map_or_else(
            || "message the agent…".to_owned(),
            |chat| format!("message {}…", chat.entry.name),
        );
        let commands = chat.map(|chat| chat.commands.clone()).unwrap_or_default();
        let streaming = chat.is_some_and(|chat| chat.streaming);
        let current = chat.and_then(|chat| agents.iter().position(|name| *name == chat.entry.name));
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
            .mx(px(8.))
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
            .child(widgets::status_dot(tone))
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

    /// The strip across the top: the traffic lights sit in its left gutter and
    /// the project tabs run along the card's edge, with the turn's controls on
    /// the far side.
    fn nav(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let chat = self.active_session();
        let streaming = chat.is_some_and(|chat| chat.streaming);
        let id = chat.map(|chat| chat.id);

        div()
            .flex_none()
            .h(px(Theme::HEADER_HEIGHT))
            .pl(px(self.sidebar_width + CARD_PAD - NAV_ITEM_PAD))
            .pr(px(CARD_PAD))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pb(px(TAB_UNDERLINE_CLEARANCE))
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap(px(2.))
                    .children(
                        self.projects
                            .iter()
                            .enumerate()
                            .map(|(ix, project)| self.tab(ix, project, cx)),
                    )
                    .child(
                        div()
                            .id("open-project")
                            .flex_none()
                            .mb(px(6.))
                            .p(px(NAV_ITEM_PAD))
                            .rounded(px(Theme::control_radius()))
                            .cursor_pointer()
                            .hover(|el| el.bg(theme.glass_hover()))
                            .tooltip(|window, cx| {
                                Tooltip::with_keystroke("Open folder", "⌘O", window, cx)
                            })
                            .child(
                                icons::icon(icons::PLUS)
                                    .size(px(13.))
                                    .text_color(theme.text_faint),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_project_action(&OpenProject, window, cx);
                            })),
                    ),
            )
            .when_some(id.filter(|_| streaming), |nav, id| {
                nav.child(
                    div()
                        .id("nav-cancel")
                        .p(px(NAV_ITEM_PAD))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.with_session(id, cx, ChatSession::cancel);
                        }))
                        .child(
                            icons::icon(icons::STOP)
                                .size(px(15.))
                                .text_color(theme.text_muted),
                        ),
                )
            })
            .when(self.active.is_some(), |nav| {
                nav.child(
                    div()
                        .id("nav-new-session")
                        .p(px(NAV_ITEM_PAD))
                        .cursor_pointer()
                        .tooltip(|window, cx| {
                            Tooltip::with_keystroke("New session", "⌘N", window, cx)
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_session_action(&NewSession, window, cx);
                        }))
                        .child(
                            icons::icon(icons::PEN_NEW_SQUARE)
                                .size(px(15.))
                                .text_color(theme.text_muted),
                        ),
                )
            })
    }

    /// One project tab, named by its directory and titled by its full path —
    /// two checkouts sharing a basename are otherwise the same tab twice.
    fn tab(&self, ix: usize, project: &Project, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let active = self.active == Some(ix);
        let path = project.path.display().to_string();
        theme
            .tab(project.name(), active)
            .id(("project", ix))
            .group("project-tab")
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .tooltip(move |window, cx| Tooltip::text(path.clone(), window, cx))
            .child(
                div()
                    .id(("close-project", ix))
                    .flex_none()
                    .invisible()
                    .group_hover("project-tab", |el| el.visible())
                    .rounded(px(Theme::control_radius()))
                    .p(px(2.))
                    .child(
                        icons::icon(icons::CLOSE)
                            .size(px(11.))
                            .text_color(theme.text_faint),
                    )
                    // Without this the tab's own click runs next and selects
                    // whichever project just slid into the closed one's index.
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_project(ix, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_project(ix, cx);
            }))
    }

    fn sidebar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let sessions = self
            .active_project()
            .map(|project| project.sessions.as_slice())
            .unwrap_or_default();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(theme.glass())
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(16.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_faint)
                    .child("SESSIONS"),
            )
            .child(
                div()
                    .id("session-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(sessions.iter().map(|chat| self.session_row(chat, cx))),
            )
    }

    fn chat(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let open = self.active_project().is_some();
        let body = match self.active_session() {
            Some(chat) => self.transcript(chat, window, cx),
            None if open => theme
                .empty_state(
                    icons::CHAT_ROUND_LINE,
                    "No session",
                    "⌘N to start one in this project.",
                )
                .flex_1()
                .into_any_element(),
            None => self.no_project(cx),
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            // Runs off the window's right and bottom edges, so the only corner
            // that floats is the one that gets rounded.
            .rounded_tl(px(Theme::panel_radius()))
            .bg(theme.surface)
            .border_t_1()
            .border_l_1()
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
            })
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
            .flex()
            .flex_col()
            .bg(theme.bg)
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_size(px(14.))
            .on_action(cx.listener(Self::new_session_action))
            .on_action(cx.listener(Self::open_project_action))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width =
                        f32::from(event.event.position.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                    cx.notify();
                }),
            )
            .child(self.nav(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(self.sidebar(cx))
                    .child(self.chat(window, cx))
                    // Rides over the card's border: a column in flow would
                    // open a seam between the rail and the card.
                    .child(
                        theme
                            .split_handle(Axis::Horizontal, SplitStyle::Ghost)
                            .id("sidebar-split")
                            .absolute()
                            .top_0()
                            .left(px(self.sidebar_width - SPLIT_HANDLE_HIT / 2.))
                            .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
                    ),
            )
    }
}
