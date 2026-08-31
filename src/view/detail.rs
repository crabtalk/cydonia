//! The detail column: whichever pane is showing, and everything the turn in
//! flight stacks under it — plan, permission, queue, composer.

use crate::{
    model::session::{ChatSession, PlanStatus},
    view::{
        component::{composer, fade, transcript},
        root::{self, Cydonia, Pane},
    },
};
use bezel::{
    gpui::{
        AnyElement, App, Context, FocusHandle, Focusable as _, FontWeight, ScrollHandle, Window,
        div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{SurfaceStyle, TextStyle, Theme, Typeset},
    ui::{
        icons,
        surface::Surfaced as _,
        widgets::{ButtonStyle, Buttons, Content, Scaffolding},
    },
};
use cacp::schema::PermissionOptionKind;
use std::path::Path;

/// Where the header's title starts. Its own measure: the panes under it do not
/// agree on one — the board insets 16, the table 24, and the conversation and
/// the article are centred columns with no left edge to meet.
const TITLE_INSET: f32 = 16.;

/// A path as it is shown: `~` for a home directory nobody needs spelled out.
fn shown_path(path: &Path) -> String {
    let full = path.display().to_string();
    dirs::home_dir()
        .and_then(|home| {
            full.strip_prefix(home.to_str()?)
                .map(|rest| format!("~{rest}"))
        })
        .unwrap_or(full)
}

impl Cydonia {
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

    pub(crate) fn submit(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(id) = self.workspace.read(cx).active_id() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.send(id, text, cx));
    }

    pub(crate) fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        self.with_active(cx, |chat| chat.cancel());
    }

    /// The composer's agent chip. An ACP session is bound to the process that
    /// serves it, so picking another agent opens a session rather than
    /// swapping one out from under a transcript.
    pub(crate) fn pick_agent(&mut self, ix: usize, cx: &mut Context<Self>) {
        let entry = self.workspace.read(cx).settings.agents.get(ix).cloned();
        if let Some(entry) = entry {
            self.show_pane(Pane::Chat, cx);
            self.workspace
                .update(cx, |workspace, cx| workspace.new_session(entry, None, cx));
        }
    }

    /// What the composer needs from the session it is pointed at: the agent's
    /// name, its commands, whether a turn is in flight, and the agents it can
    /// be swapped for.
    pub(crate) fn sync_composer(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn detail(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let open = self.workspace.read(cx).active_project().is_some();
        // Nothing to send to: the session's agent is gone from settings.toml,
        // so there is nothing left to reconnect it to.
        let live = self
            .workspace
            .read(cx)
            .active_session()
            .is_none_or(ChatSession::resumable);
        let showing = self.showing(cx);
        let body = if !open {
            self.no_project(cx)
        } else {
            match showing {
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

        let content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(body)
            .when(open && live && showing == Pane::Chat, |content| {
                content.child(
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
            .relative()
            .bg(root::frost(&theme, root::CONTENT_FROST))
            .flex()
            .flex_col()
            .child(fade::under(
                root::HEADER_HEIGHT,
                Theme::TRANSCRIPT_FADE_BAND,
                self.pane_scroll(cx),
                content,
            ))
            // Last, and out of flow: the pane's scroll starts at y=0 and
            // reserves the header's room as padding of its own, so what you
            // scroll passes under the header rather than stopping at it.
            .child(self.pane_header(window, cx))
    }

    /// What the pane is showing, named. The sidebar says the same thing while
    /// it is open, and is the only thing that does once it is not.
    fn pane_header(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // Folded away, this column is the one on the window's left edge, so it
        // takes over the band the traffic lights float in — and the toggle
        // standing in it.
        let bar = match self.sidebar_open {
            true => div()
                .flex_none()
                .h(px(root::HEADER_HEIGHT))
                .pl(px(TITLE_INSET))
                .pr(px(8.))
                .flex()
                .flex_row()
                .items_center(),
            false => self.toolbar(window, cx),
        };
        let header = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .child(bar.gap(px(8.)).children(self.pane_title(cx).map(|title| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Callout)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(title)
            })));
        match self.under_header(cx) {
            true => header
                .surface(&theme, SurfaceStyle::Frost(root::HEADER_FROST))
                .into_any_element(),
            // Nothing of its own: the fade has already taken the content away
            // before it reaches here, and a material with nothing behind it is
            // a second coat of the column rather than a view through it.
            false => header.into_any_element(),
        }
    }

    /// Whether the pane has put anything behind the header. Only the article
    /// does: its cover runs to the top of the column, where every other pane
    /// either reserves the room or has had its content faded out.
    fn under_header(&self, cx: &Context<Self>) -> bool {
        matches!(self.showing(cx), Pane::Article)
            && self.workspace.read(cx).active_article().is_some()
    }

    /// The scroll the fade is gated on. Only the transcript: the article's
    /// cover is one tall sprite, and a sprite crossing the ramp goes entirely
    /// rather than by degrees, so the fade is for text and small marks. The
    /// board scrolls across rather than up, and the table's headings sit above
    /// its own scroll.
    fn pane_scroll(&self, cx: &Context<Self>) -> Option<ScrollHandle> {
        match self.showing(cx) {
            Pane::Chat => self
                .workspace
                .read(cx)
                .active_session()
                .map(|chat| chat.transcript.scroll.clone()),
            Pane::Article | Pane::Board | Pane::Table => None,
        }
    }

    /// The name of whatever is in front, as the sidebar spells it.
    fn pane_title(&self, cx: &Context<Self>) -> Option<String> {
        let showing = self.showing(cx);
        let workspace = self.workspace.read(cx);
        workspace.active_project()?;
        match showing {
            Pane::Chat => workspace.active_session().map(ChatSession::label),
            Pane::Board => workspace
                .active_board()
                .map(|board| board.label().to_owned()),
            Pane::Article => workspace
                .active_article()
                .map(|article| article.label().to_owned()),
            Pane::Table => workspace.active_table().map(|table| table.name.clone()),
        }
    }

    /// The session in front, or the invitation to open one.
    fn conversation(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(chat) = workspace.active_session() else {
            return theme
                .empty_state(
                    icons::CHAT_ROUND_LINE,
                    "No session",
                    "⌘N to start one in this project.",
                )
                .flex_1()
                .into_any_element();
        };
        // Nothing has been said yet, so what the session has to show for
        // itself is the directory the agent was started in.
        if chat.items.is_empty() {
            let cwd = workspace
                .active_project()
                .map(|project| shown_path(&project.path))
                .unwrap_or_default();
            return theme
                .empty_state(icons::FOLDER, cwd, format!("{} runs here", chat.entry.name))
                .flex_1()
                .into_any_element();
        }
        let id = chat.id;
        self.workspace
            .update(cx, |workspace, cx| match workspace.session(id) {
                Some(chat) => transcript::render(chat, window, cx),
                None => div().flex_1().into_any_element(),
            })
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
                .text_style(TextStyle::Callout)
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
                        .text_style(TextStyle::Body)
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
