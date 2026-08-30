//! Root view: the projects rail, and the chat column beside it.

use crate::{
    model::{
        project::Project,
        session::{ChatSession, PlanStatus},
        settings::Settings,
        state::State,
        workspace::Workspace,
    },
    view::{
        board::{self, Editing},
        composer::{self, Composer, ComposerEvent},
        settings_window::{self, SettingsWindow},
        transcript,
    },
};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable as _,
        FontWeight, KeyBinding, PathPromptOptions, Render, Window, WindowHandle, div, prelude::*,
        px, svg,
    },
    motion::{Fade, Painter},
    theme::Theme,
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

/// The root view. It owns no app state — only the chrome's own: how wide the
/// rail is, which pane is showing, and whichever card is being written.
pub struct Cydonia {
    pub(crate) workspace: Entity<Workspace>,
    sidebar_width: f32,
    composer: Entity<Composer>,
    settings_window: Option<WindowHandle<SettingsWindow>>,
    /// Which pane the content card shows. A property of the window, not of a
    /// project — switching projects must not teleport you to the other pane.
    pub(crate) board_open: bool,
    pub(crate) editing: Option<Editing>,
    pub(crate) card_field: Entity<TextField>,
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
        let workspace = cx.new(|cx| Workspace::new(settings, state, cx));
        // The model is the only thing that says a session appeared or a turn
        // ended; the composer's placeholder, commands and busy state are all
        // read back from it rather than pushed by whoever caused the change.
        cx.observe(&workspace, |this, _, cx| this.sync_composer(cx))
            .detach();

        let mut this = Self {
            workspace,
            sidebar_width: SIDEBAR_DEFAULT,
            composer,
            settings_window: None,
            board_open: false,
            editing: None,
            card_field,
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
            self.workspace
                .update(cx, |workspace, cx| workspace.new_session(entry, None, cx));
        }
    }

    fn new_session_action(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            if let Some(entry) = workspace.preferred_agent() {
                workspace.new_session(entry, None, cx);
            }
        });
    }

    /// Leaving a project is the moment a half-written card has to be filed:
    /// the spot it points at belongs to the board being navigated away from.
    pub(crate) fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_project(ix, cx));
    }

    pub(crate) fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit_edit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.close_project(ix, cx));
    }

    pub(crate) fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
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

    fn session_row(&self, chat: &ChatSession, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let id = chat.id;
        let selected = self.workspace.read(cx).active_id() == Some(id);
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
                    .child(match self.workspace.read(cx).agent_icon(&chat.entry.name) {
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
        let active = self.workspace.read(cx).active == Some(ix);
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
        let sections: Vec<AnyElement> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .enumerate()
            .map(|(ix, project)| self.project_section(ix, project, cx).into_any_element())
            .collect();
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

    fn chat(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let open = self.workspace.read(cx).active_project().is_some();
        let body = if !open {
            self.no_project(cx)
        } else if self.board_open {
            self.board(cx)
        } else {
            match self.workspace.read(cx).active_id() {
                Some(id) => {
                    self.workspace
                        .update(cx, |workspace, cx| match workspace.session(id) {
                            Some(chat) => transcript::render(chat, window, cx),
                            None => div().flex_1().into_any_element(),
                        })
                }
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
