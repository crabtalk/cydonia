//! The detail column: whichever pane is showing, and everything the turn in
//! flight stacks under it — plan, permission, queue, composer.

use crate::{
    model::session::{ChatSession, PlanStatus},
    view::{
        component::{composer, transcript},
        root::{self, Cydonia, NewSession, Pane},
    },
};
use bezel::{
    gpui::{AnyElement, App, Context, FocusHandle, Focusable as _, Window, div, prelude::*, px},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons, Content, Scaffolding},
    },
};
use cacp::schema::PermissionOptionKind;
use std::path::Path;

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
        // Nothing to send to: no session at all, or one whose agent has gone
        // from settings.toml, leaving nothing to reconnect it to.
        let live = self
            .workspace
            .read(cx)
            .active_session()
            .is_some_and(ChatSession::resumable);
        let showing = self.showing(cx);
        let body = match showing {
            None => self.launch(cx),
            Some(Pane::Chat) => self.conversation(window, cx),
            Some(Pane::Board) => self.board(cx),
            // An entry can be named and not yet loaded — an article holds no
            // editor until it is opened. The front door stands in for the
            // moment in between.
            Some(Pane::Article) => self.article(cx).unwrap_or_else(|| self.launch(cx)),
            Some(Pane::Table) => self.table(cx).unwrap_or_else(|| self.launch(cx)),
        };

        let content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(body);

        div()
            .flex_1()
            .min_w_0()
            .relative()
            .bg(root::content_bg(&theme))
            .flex()
            .flex_col()
            .child(content)
            // Out of flow so the transcript runs under it: the composer's glass
            // has something to bend only where the messages reach its edge.
            .when(live && showing == Some(Pane::Chat), |column| {
                column.child(
                    div()
                        .absolute()
                        .bottom(px(root::COMPOSER_BOTTOM))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w_full()
                                .max_w(px(720.))
                                .px(px(24.))
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
            // Out of flow, so folding the sidebar away costs the pane nothing:
            // the controls float on the column rather than taking a row off it.
            .when(!self.sidebar_open, |column| {
                column.child(self.fold_cluster(window, cx))
            })
    }
}

/// The invitation's rows as one block: left-aligned so every glyph lands on the
/// same edge, and held off the line above it — `empty_state` centres its
/// children, which would otherwise centre each row on its own width.
fn make_list(rows: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    div()
        .mt(px(14.))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(8.))
        .children(rows)
}

impl Cydonia {
    /// The front door, and what stands where a pane would be if one were
    /// showing: a project to open, or the first entry to make in the one that
    /// already is.
    fn launch(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.workspace.read(cx).active_project().is_some() {
            true => self.nothing_open(cx),
            false => self.no_project(cx),
        }
    }

    /// What to do when there is nothing to show: make the first entry in the
    /// project that is open. The kinds are listed rather than named in a hint,
    /// because a list can be clicked — and only the kinds that are switched on
    /// are listed, so under the shipped defaults this is one line.
    fn nothing_open(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(ix) = workspace.active else {
            return self.no_project(cx);
        };
        let features = &workspace.settings.features;
        let (sessions, boards, tables) = (features.sessions, features.boards, features.tables);
        let name = workspace
            .projects
            .get(ix)
            .map(|project| shown_path(&project.path))
            .unwrap_or_default();
        let mut rows: Vec<AnyElement> = Vec::new();
        if sessions {
            rows.push(self.make_row(
                "session",
                "New session",
                icons::system::CHAT_ROUND_LINE,
                cx,
                move |this, window, cx| this.new_session_action(&NewSession, window, cx),
            ));
        }
        if boards {
            rows.push(
                self.make_row("board", "New board", icons::editing::LIST, cx, move |this, _, cx| {
                    this.new_board(ix, cx)
                }),
            );
        }
        rows.push(self.make_row(
            "article",
            "New article",
            icons::files::DOCUMENT_ADD,
            cx,
            move |this, window, cx| this.new_article(ix, window, cx),
        ));
        if tables {
            rows.push(self.make_row(
                "table",
                "New table",
                icons::system::WIDGET,
                cx,
                move |this, _, cx| this.new_table(ix, cx),
            ));
        }
        theme
            .empty_state(icons::files::FOLDER, "Nothing open", format!("in {name}"))
            .flex_1()
            .child(make_list(rows))
            .into_any_element()
    }

    /// One line of the invitation: a glyph, a label, and what it makes.
    fn make_row(
        &self,
        id: &'static str,
        label: &'static str,
        glyph: &'static str,
        cx: &mut Context<Self>,
        make: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // An svg paints in its own `text_color` and inherits none, so the glyph
        // cannot ride the row's hover. Both halves take the row's group instead,
        // which lights them together — the group is named per row so hovering
        // one does not light the rest.
        div()
            .id(id)
            .group(id)
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_style(TextStyle::Callout)
            .child(
                icons::icon(glyph)
                    .size(px(14.))
                    .flex_none()
                    .text_color(theme.text_muted)
                    .group_hover(id, |el| el.text_color(theme.text)),
            )
            .child(
                div()
                    .text_color(theme.text_muted)
                    .group_hover(id, |el| el.text_color(theme.text))
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| make(this, window, cx)))
            .into_any_element()
    }

    /// The session in front. It has one: the chat pane is named by `showing`
    /// only where a session is open in it, so the empty case is unreachable
    /// rather than a state this has to draw.
    fn conversation(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(chat) = workspace.active_session() else {
            return div().flex_1().into_any_element();
        };
        // Nothing has been said yet, so what the session has to show for
        // itself is the directory the agent was started in.
        if chat.items.is_empty() {
            let cwd = workspace
                .active_project()
                .map(|project| shown_path(&project.path))
                .unwrap_or_default();
            return theme
                .empty_state(icons::files::FOLDER, cwd, format!("{} runs here", chat.entry.name))
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
                        PlanStatus::Done => (icons::status::CHECK, theme.success),
                        PlanStatus::Active => (icons::arrows::ALT_ARROW_RIGHT, theme.accent),
                        PlanStatus::Pending => (icons::editing::CHECKLIST, theme.text_faint),
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
