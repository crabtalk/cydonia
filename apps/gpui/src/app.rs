//! Root view: left sidebar | chat column.

use crate::composer::{Composer, ComposerEvent};
use crate::session::{ChatSession, PlanStatus};
use crate::theme::Theme;
use crate::transcript::transcript;
use cydonia_core::settings::{self, Settings};
use gpui::{
    Context, DragMoveEvent, Empty, Entity, FocusHandle, Focusable as _, Render, Window, actions,
    div, prelude::*, px,
};

actions!(cydonia, [CancelTurn, NewSession]);

const SIDEBAR_DEFAULT: f32 = 240.;
const SIDEBAR_MIN: f32 = 180.;
const SIDEBAR_MAX: f32 = 420.;
/// Clearance under the macOS traffic lights.
const TITLEBAR_HEIGHT: f32 = 40.;

pub struct Cydonia {
    pub settings: Settings,
    sessions: Vec<ChatSession>,
    active: Option<u64>,
    next_id: u64,
    sidebar_width: f32,
    composer: Entity<Composer>,
}

struct SidebarResize;

struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Cydonia {
    pub fn new(settings: Settings, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(Composer::new);
        cx.subscribe(
            &composer,
            |this, _, event: &ComposerEvent, cx| match event {
                ComposerEvent::Submit(text) => this.submit(text.clone(), cx),
            },
        )
        .detach();

        let mut this = Self {
            settings,
            sessions: Vec::new(),
            active: None,
            next_id: 0,
            sidebar_width: SIDEBAR_DEFAULT,
            composer,
        };
        if let Some(entry) = this.settings.agents.first().cloned() {
            this.new_session(entry, cx);
        }
        this
    }

    pub fn composer_focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.composer.focus_handle(cx)
    }

    fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(id) = self.active {
            self.with_session(id, cx, |chat| chat.send(text));
        }
    }

    fn cancel_turn(&mut self, _: &CancelTurn, _: &mut Window, cx: &mut Context<Self>) {
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
        let placeholder = self
            .session(id)
            .map(|chat| format!("message {}…", chat.entry.name))
            .unwrap_or_default();
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx)
        });
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
        let placeholder = format!("message {}…", entry.name);
        self.sessions.push(ChatSession::connect(id, entry, cwd, cx));
        self.active = Some(id);
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx)
        });
        cx.notify();
    }

    pub fn session(&self, id: u64) -> Option<&ChatSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Run `f` on the session (when it still exists), sync its list, and
    /// repaint.
    pub fn with_session(
        &mut self,
        id: u64,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ChatSession),
    ) {
        if let Some(chat) = self.sessions.iter_mut().find(|s| s.id == id) {
            f(chat);
            chat.sync_list();
            cx.notify();
        }
    }

    pub fn set_scrolled(&mut self, id: u64, scrolled: bool, cx: &mut Context<Self>) {
        if let Some(chat) = self.sessions.iter_mut().find(|s| s.id == id)
            && chat.scrolled != scrolled
        {
            chat.scrolled = scrolled;
            cx.notify();
        }
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

    fn render_session_row(
        &self,
        chat: &ChatSession,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx);
        let id = chat.id;
        let selected = self.active == Some(id);
        let label = if chat.title.is_empty() {
            chat.entry.name.clone()
        } else {
            chat.title.clone()
        };
        let dot_color = if chat.lost {
            theme.danger
        } else if chat.streaming {
            theme.accent
        } else if chat.session.is_some() {
            theme.success
        } else {
            theme.text_tertiary
        };

        div()
            .id(("session", id))
            .group("session-row")
            .mx_2()
            .px_2()
            .py_1p5()
            .rounded_md()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.code_wash))
            .hover(|el| el.bg(theme.code_wash))
            .child(div().flex_none().size(px(7.)).rounded_full().bg(dot_color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_size(px(13.))
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_secondary
                    })
                    .child(label),
            )
            .child(
                div()
                    .id(("close", id))
                    .flex_none()
                    .invisible()
                    .group_hover("session-row", |el| el.visible())
                    .px_1()
                    .rounded_sm()
                    .text_size(px(12.))
                    .text_color(theme.text_tertiary)
                    .hover(|el| el.text_color(theme.text))
                    .child("×")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.close_session(id, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.select_session(id, cx);
            }))
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx);
        div()
            .relative()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .pt(px(TITLEBAR_HEIGHT))
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .pb_1()
                    .text_size(px(11.))
                    .text_color(theme.text_tertiary)
                    .child("sessions"),
            )
            .child(
                div()
                    .id("session-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(
                        self.sessions
                            .iter()
                            .map(|chat| self.render_session_row(chat, cx)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .p_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(self.settings.agents.clone().into_iter().enumerate().map(
                        |(ix, entry)| {
                            let name = entry.name.clone();
                            div()
                                .id(("new-session", ix))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_size(px(13.))
                                .text_color(theme.text_secondary)
                                .cursor_pointer()
                                .hover(|el| el.bg(theme.code_wash))
                                .child(format!("+ {name}"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.new_session(entry.clone(), cx);
                                }))
                        },
                    )),
            )
            .child(
                div()
                    .id("sidebar-resize")
                    .absolute()
                    .top_0()
                    .right(px(-3.))
                    .w(px(6.))
                    .h_full()
                    .cursor_col_resize()
                    .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| DragGhost)),
            )
    }

    fn render_plan(&self, chat: &ChatSession, theme: Theme) -> Option<impl IntoElement + use<>> {
        if chat.plan.is_empty() || chat.plan.iter().all(|(_, s)| *s == PlanStatus::Done) {
            return None;
        }
        Some(
            div()
                .flex_none()
                .mx_4()
                .mb_2()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(theme.raised)
                .border_1()
                .border_color(theme.border)
                .text_size(px(12.))
                .children(chat.plan.iter().map(|(text, status)| {
                    let (glyph, color) = match status {
                        PlanStatus::Done => ("✓", theme.success),
                        PlanStatus::Active => ("◐", theme.accent),
                        PlanStatus::Pending => ("○", theme.text_tertiary),
                    };
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .text_color(theme.text_secondary)
                        .child(div().flex_none().text_color(color).child(glyph))
                        .child(text.clone())
                })),
        )
    }

    fn render_chat(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx);
        let chat_area = match self.active_session() {
            Some(chat) => transcript(chat, cx.entity(), theme),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.text_tertiary)
                .child("no session — ⌘N or pick an agent in the sidebar")
                .into_any_element(),
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface)
            .pt(px(TITLEBAR_HEIGHT))
            .child(chat_area)
            .children(
                self.active_session()
                    .and_then(|chat| self.render_plan(chat, theme)),
            )
            .children(self.render_permission(cx))
            .children(self.render_queue(theme))
            .child(self.render_composer(cx))
    }

    /// The agent's tool-authorization request, one button per option.
    fn render_permission(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx);
        let chat = self.active_session()?;
        let prompt = chat.permission.as_ref()?;
        let id = chat.id;
        Some(
            div()
                .flex_none()
                .mx_4()
                .mb_2()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(theme.raised)
                .border_1()
                .border_color(theme.accent)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(prompt.title.clone()),
                )
                .child(
                    div().flex().flex_row().gap_2().children(
                        prompt
                            .options
                            .iter()
                            .enumerate()
                            .map(|(ix, (option_id, name))| {
                                let option_id = option_id.clone();
                                div()
                                    .id(("permission-option", ix))
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.code_wash)
                                    .border_1()
                                    .border_color(theme.border)
                                    .text_size(px(12.))
                                    .text_color(theme.text_secondary)
                                    .cursor_pointer()
                                    .hover(|el| el.bg(theme.selection).text_color(theme.text))
                                    .child(name.clone())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let option_id = option_id.clone();
                                        this.with_session(id, cx, |chat| {
                                            chat.respond_permission(option_id);
                                        });
                                    }))
                            }),
                    ),
                ),
        )
    }

    /// Prompts waiting for the in-flight turn, as dim chips.
    fn render_queue(&self, theme: Theme) -> Option<impl IntoElement + use<>> {
        let chat = self.active_session()?;
        if chat.queue.is_empty() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .mx_4()
                .mb_1()
                .flex()
                .flex_col()
                .gap_1()
                .children(chat.queue.iter().map(|text| {
                    div()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(theme.code_wash)
                        .text_size(px(12.))
                        .text_color(theme.text_tertiary)
                        .child(format!("queued · {text}"))
                })),
        )
    }

    fn render_composer(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx);
        let streaming = self.active_session().is_some_and(|chat| chat.streaming);
        let empty = self.composer.read(cx).is_empty();
        let (label, color) = if streaming {
            ("stop", theme.danger)
        } else if empty {
            ("send", theme.text_tertiary)
        } else {
            ("send", theme.accent)
        };
        let composer = self.composer.clone();

        div()
            .flex_none()
            .mx_4()
            .mb_4()
            .p_3()
            .rounded_lg()
            .bg(theme.composer)
            .border_1()
            .border_color(theme.border)
            .flex()
            .flex_row()
            .items_end()
            .gap_3()
            .child(self.composer.clone())
            .when(streaming, |el| {
                el.child(
                    div()
                        .flex_none()
                        .pb_0p5()
                        .text_size(px(12.))
                        .text_color(theme.text_tertiary)
                        .child("working…"),
                )
            })
            .child(
                div()
                    .id("send-stop")
                    .flex_none()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(theme.raised)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(12.))
                    .text_color(color)
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.active_session().is_some_and(|chat| chat.streaming) {
                            if let Some(id) = this.active {
                                this.with_session(id, cx, |chat| chat.cancel());
                            }
                        } else {
                            composer.update(cx, |composer, cx| composer.submit(cx));
                        }
                    })),
            )
    }
}

impl Render for Cydonia {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx);
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(theme.canvas)
            .text_color(theme.text)
            .text_size(px(14.))
            .on_action(cx.listener(Self::cancel_turn))
            .on_action(cx.listener(Self::new_session_action))
            .on_drag_move::<SidebarResize>(cx.listener(
                |this, event: &DragMoveEvent<SidebarResize>, _, cx| {
                    this.sidebar_width =
                        f32::from(event.event.position.x).clamp(SIDEBAR_MIN, SIDEBAR_MAX);
                    cx.notify();
                },
            ))
            .child(self.render_sidebar(cx))
            .child(self.render_chat(cx))
    }
}
