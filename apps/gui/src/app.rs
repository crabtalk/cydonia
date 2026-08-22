//! Root view: the sessions rail, and the chat column beside it.

use crate::{
    composer::{Composer, ComposerEvent},
    session::{ChatSession, PlanStatus},
};
use bezel::{
    gpui::{
        App, Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable as _, FontWeight,
        KeyBinding, MouseButton, Render, SharedString, Window, div, prelude::*, px,
    },
    theme::Theme,
    ui::{
        icons, widgets,
        widgets::{ButtonStyle, Buttons, Content, Layout, Scaffolding, SplitDrag},
    },
};
use cydonia_core::{
    acp::schema::v1::PermissionOptionKind,
    settings::{self, Settings},
};
use gpui::actions;

actions!(cydonia, [NewSession]);

const SIDEBAR_DEFAULT: f32 = 240.;
const SIDEBAR_MIN: f32 = 180.;
const SIDEBAR_MAX: f32 = 420.;

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-n", NewSession, None)]);
}

pub struct Cydonia {
    pub settings: Settings,
    sessions: Vec<ChatSession>,
    active: Option<u64>,
    next_id: u64,
    sidebar_width: f32,
    dragging: bool,
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
            },
        )
        .detach();

        let mut this = Self {
            settings,
            sessions: Vec::new(),
            active: None,
            next_id: 0,
            sidebar_width: SIDEBAR_DEFAULT,
            dragging: false,
            composer,
        };
        if let Some(entry) = this.settings.agents.first().cloned() {
            this.new_session(entry, cx);
        }
        this
    }

    pub fn composer_focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer.focus_handle(cx)
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            self.with_session(id, cx, |chat| chat.send(text));
        }
    }

    fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            self.with_session(id, cx, |chat| chat.cancel());
        }
    }

    fn new_session_action(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.settings.agents.first().cloned() {
            self.new_session(entry, cx);
        }
    }

    pub fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.session(id).is_none() || self.active == Some(id) {
            return;
        }
        self.active = Some(id);
        self.sync_composer(cx);
        cx.notify();
    }

    /// Drop the session: the shutdown sender goes with it and the agent
    /// process dies.
    pub fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.sessions.retain(|chat| chat.id != id);
        if self.active == Some(id) {
            let next = self.sessions.last().map(|chat| chat.id);
            self.active = None;
            if let Some(next) = next {
                self.select_session(next, cx);
            }
        }
        cx.notify();
    }

    pub fn new_session(&mut self, entry: settings::Agent, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let cwd = std::env::current_dir()
            .or_else(|_| dirs::home_dir().ok_or(std::io::Error::other("no home")))
            .unwrap_or_else(|_| "/".into());
        self.sessions.push(ChatSession::connect(id, entry, cwd, cx));
        self.active = Some(id);
        self.sync_composer(cx);
        cx.notify();
    }

    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Run `f` on the session (when it still exists) and repaint.
    pub fn with_session(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ChatSession),
    ) {
        if let Some(chat) = self.sessions.iter_mut().find(|s| s.id == id) {
            f(chat);
            if self.active == Some(id) {
                self.sync_composer(cx);
            }
            cx.notify();
        }
    }

    /// What the composer needs from the session it is pointed at: the agent's
    /// name, its commands, and whether a turn is in flight.
    fn sync_composer(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.active_session() else {
            return;
        };
        let placeholder = format!("message {}…", chat.entry.name);
        let commands = chat.commands.clone();
        let streaming = chat.streaming;
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx);
            composer.set_commands(&commands, cx);
            composer.set_streaming(streaming, cx);
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
        self.active.and_then(|id| self.session(id))
    }

    fn session_row(&self, chat: &ChatSession, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let id = chat.id;
        let selected = self.active == Some(id);
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
            .rounded(px(Theme::CONTROL_RADIUS))
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
                    .rounded(px(Theme::CONTROL_RADIUS))
                    .p(px(2.))
                    .child(
                        icons::icon(icons::CLOSE)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_session(id, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(id, cx);
            }))
    }

    fn sidebar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(theme.glass())
            .pt(px(Theme::TITLEBAR_HEIGHT))
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
                    .children(self.sessions.iter().map(|chat| self.session_row(chat, cx))),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(8.))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(self.settings.agents.clone().into_iter().enumerate().map(
                        |(ix, entry)| {
                            let name = entry.name.clone();
                            div()
                                .id(("new-session", ix))
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(Theme::CONTROL_RADIUS))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(6.))
                                .cursor_pointer()
                                .hover(|el| el.bg(theme.glass_hover()))
                                .child(
                                    icons::icon(icons::PLUS)
                                        .size(px(12.))
                                        .text_color(theme.text_faint),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(theme.text_muted)
                                        .child(name),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.new_session(entry.clone(), cx);
                                }))
                        },
                    )),
            )
    }

    fn chat(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let body = match self.active_session() {
            Some(chat) => self.transcript(chat, window, cx),
            None => theme
                .empty_state(
                    icons::CHAT_ROUND_LINE,
                    "No session",
                    "⌘N, or pick an agent in the sidebar.",
                )
                .flex_1()
                .into_any_element(),
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.bg)
            .pt(px(Theme::TITLEBAR_HEIGHT))
            .child(body)
            .child(
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
                        let fade = SharedString::from(format!("permission-{id}-{ix}"));
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
            .flex_row()
            .bg(theme.bg)
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_size(px(14.))
            .on_action(cx.listener(Self::new_session_action))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width =
                        f32::from(event.event.position.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                    this.dragging = true;
                    cx.notify();
                }),
            )
            // Both, because the release can land anywhere: a divider left lit
            // reads as still grabbed.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dragging = false;
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dragging = false;
                    cx.notify();
                }),
            )
            .child(self.sidebar(cx))
            .child(
                theme
                    .split_handle(Axis::Horizontal, self.dragging)
                    .id("sidebar-split")
                    .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
            )
            .child(self.chat(window, cx))
    }
}
