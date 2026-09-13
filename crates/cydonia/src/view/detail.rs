//! The detail column: whichever pane is showing, and everything the turn in
//! flight stacks under it — plan, permission, queue, composer.

use crate::{
    model::session::{ChatSession, Choice},
    view::{
        component::{composer, transcript},
        root::{self, Cydonia, NewSession, Pane},
    },
};
use artifact::session::chat::PlanStatus;
use bezel::{
    gpui::{
        AnyElement, App, Context, FocusHandle, Focusable as _, SharedString, Window, div,
        prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, surface,
        widgets::{ButtonStyle, Buttons, Content, Controls},
    },
};
use cacp::schema::{
    PermissionOptionKind, SessionConfigKind, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionModeState,
};
use std::path::Path;
use surface::Surfaced as _;

/// What the live session can be switched between, flattened to the one shape
/// the composer draws: its config options, then its modes.
///
/// Config first, because the model is a config option and it is the one people
/// came for. Booleans are left out — a menu of two rows is a toggle wearing a
/// menu, and the agent menu has nowhere to put a real one yet.
fn switches(chat: &ChatSession) -> Vec<composer::Switch> {
    let mut switches: Vec<composer::Switch> = chat
        .config
        .iter()
        .filter_map(|option| {
            let SessionConfigKind::Select(select) = &option.kind else {
                return None;
            };
            let options = select_options(&select.options);
            // A select with nothing to select is a menu with no rows.
            (!options.is_empty()).then(|| composer::Switch {
                id: composer::SwitchId::Config(option.id.to_string().into()),
                name: option.name.clone().into(),
                current: Some(select.current_value.to_string().into()),
                options,
            })
        })
        .collect();
    if let Some(modes) = chat
        .modes
        .as_ref()
        .filter(|modes| !modes.available_modes.is_empty())
        .filter(|modes| !covered_by_config(chat, modes))
    {
        switches.push(composer::Switch {
            id: composer::SwitchId::Mode,
            name: "Mode".into(),
            current: Some(modes.current_mode_id.to_string().into()),
            options: modes
                .available_modes
                .iter()
                .map(|mode| composer::SwitchOption {
                    id: mode.id.to_string().into(),
                    name: mode.name.clone().into(),
                })
                .collect(),
        });
    }
    switches
}

/// Whether the agent is already offering these modes as a config option.
///
/// Some agents report their modes twice — once through `session/new`'s `modes`
/// and again as a config option — and two switches onto one piece of state is
/// two ways to disagree about it. The config option wins: it is the general
/// mechanism, and its update is what confirms a change.
///
/// Matched on the values as well as on the category, because the category is
/// optional and an agent that leaves it off still sends the same list twice.
fn covered_by_config(chat: &ChatSession, modes: &SessionModeState) -> bool {
    chat.config.iter().any(|option| {
        if option.category == Some(SessionConfigOptionCategory::Mode) {
            return true;
        }
        let SessionConfigKind::Select(select) = &option.kind else {
            return false;
        };
        let values = select_options(&select.options);
        modes
            .available_modes
            .iter()
            .all(|mode| values.iter().any(|value| value.id.as_ref() == &*mode.id))
    })
}

/// A select's values, with a group's rows folded in beside the ungrouped ones.
/// A switch gets one flat card, and a group is a heading it has nowhere to put.
fn select_options(options: &SessionConfigSelectOptions) -> Vec<composer::SwitchOption> {
    fn one(option: &SessionConfigSelectOption) -> composer::SwitchOption {
        composer::SwitchOption {
            id: option.value.to_string().into(),
            name: option.name.clone().into(),
        }
    }
    match options {
        SessionConfigSelectOptions::Ungrouped(options) => options.iter().map(one).collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter().map(one))
            .collect(),
    }
}

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

/// One of the two answers a permission request comes down to, with the *once*
/// and *always* forms of it read as one.
struct Verdict<'a> {
    /// What the button says: the once-form's own label, whatever the checkbox
    /// is set to. Agents word the always-form as a sentence — "Yes, and allow
    /// access to repos/ and ls commands" — and a sentence is not a button.
    label: &'a str,
    once: &'a str,
    always: Option<&'a str>,
}

impl Verdict<'_> {
    /// The option id this answer sends with the checkbox in that state. An
    /// always with no form to send falls back to the once: an agent offering
    /// "allow always" and no "reject always" still has to be refusable.
    fn id(&self, always: bool) -> String {
        match always {
            true => self.always.unwrap_or(self.once).to_owned(),
            false => self.once.to_owned(),
        }
    }
}

/// The request as an alert — a yes, a no, and a checkbox — or `None` when it
/// is not one.
///
/// ACP's four option kinds are two answers times "for how long", which is the
/// macOS permission alert exactly. It holds only while the two sides account
/// for every option the agent sent: the count is what catches a second option
/// of a kind already taken, and a kind we do not know. Dropping something the
/// agent asked about is not ours to do, so anything else falls to the stack.
fn alert(options: &[Choice]) -> Option<(Verdict<'_>, Verdict<'_>)> {
    let deny = verdict(options, false)?;
    let allow = verdict(options, true)?;
    let covered = 2 + usize::from(deny.always.is_some()) + usize::from(allow.always.is_some());
    (covered == options.len()).then_some((deny, allow))
}

/// One side of the request, if the agent offered its once-form. Without one
/// there is no button to put the checkbox under.
fn verdict(options: &[Choice], allow: bool) -> Option<Verdict<'_>> {
    let (once, ever) = match allow {
        true => (
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ),
        false => (
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ),
    };
    let of = |kind: PermissionOptionKind| options.iter().find(move |o| o.kind == kind);
    let once = of(once)?;
    Some(Verdict {
        label: &once.name,
        once: &once.id,
        always: of(ever).map(|option| option.id.as_str()),
    })
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
        // Both belong to the agent process rather than to the transcript, so a
        // session read back off disk offers neither until it reconnects.
        let live = chat.filter(|chat| chat.live());
        let switches = live.map(switches).unwrap_or_default();
        let usage = live.and_then(|chat| chat.usage);
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(&placeholder, cx);
            composer.set_commands(&commands, cx);
            composer.set_streaming(streaming, cx);
            composer.set_agents(&agents, current, cx);
            composer.set_switches(&switches, cx);
            composer.set_usage(usage, cx);
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
            // The band is drawn over the column, so the room it needs is taken
            // here. Held on the pane rather than inside each pane's scroll,
            // which is where it wants to end up — `../desktop` reserves it
            // inside the scroll so content slides under the glass, and doing
            // that means every pane's own scroll box, not this one div.
            .pt(px(root::HEADER_HEIGHT))
            .child(body);

        div()
            .flex_1()
            .min_w_0()
            .relative()
            .bg(root::content_bg(&theme))
            .flex()
            .flex_col()
            .child(content)
            // After the content, so it draws over it.
            .child(self.pane_header(window, cx))
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
                icons::social::MessageCircle,
                cx,
                move |this, window, cx| this.new_session_action(&NewSession, window, cx),
            ));
        }
        if boards {
            rows.push(self.make_row(
                "board",
                "New board",
                icons::text::List,
                cx,
                move |this, window, cx| this.ask_new_board(ix, window, cx),
            ));
        }
        rows.push(self.make_row(
            "article",
            "New article",
            icons::files::FilePlus,
            cx,
            move |this, window, cx| this.new_article(ix, window, cx),
        ));
        if tables {
            rows.push(self.make_row(
                "table",
                "New table",
                icons::layout::LayoutGrid,
                cx,
                move |this, _, cx| this.new_table(ix, cx),
            ));
        }
        theme
            .empty_state(icons::files::Folder, "Nothing open", format!("in {name}"))
            .flex_1()
            .child(make_list(rows))
            .into_any_element()
    }

    /// One line of the invitation: a glyph, a label, and what it makes.
    fn make_row(
        &self,
        id: &'static str,
        label: &'static str,
        glyph: &'static [u8],
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
                .empty_state(
                    icons::files::Folder,
                    cwd,
                    format!("{} runs here", chat.entry.name),
                )
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
            div()
                .rounded(px(Theme::surface_radius()))
                .px(px(12.))
                .py(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .text_style(TextStyle::Callout)
                .children(chat.plan.iter().map(|(text, status)| {
                    let (icon, tone) = match status {
                        PlanStatus::Done => (icons::notifications::Check, theme.success),
                        PlanStatus::Active => (icons::arrows::ChevronRight, theme.accent),
                        PlanStatus::Pending => (icons::text::ListChecks, theme.text_faint),
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap(px(8.))
                        .text_color(theme.text_muted)
                        .child(icons::icon(icon).size(px(12.)).text_color(tone))
                        .child(text.clone())
                }))
                .surface(&theme, composer::SURFACE),
        )
    }

    /// The agent's tool-authorization request.
    ///
    /// A macOS permission alert: what is being asked for, the two answers, and
    /// a checkbox saying how long the answer holds. See [`alert`] for why two
    /// buttons carry four options, and for what an agent has to ask to get the
    /// stack of rows instead.
    fn permission(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        let prompt = chat.permission.as_ref()?;
        let id = chat.id;
        let painter = Painter::of(cx);
        // One button, whichever layout it lands in. `key` is the element's and
        // the hover wash's both — the wash store is one map for the whole app,
        // so the session is in it too.
        let answer = |key: &str, option_id: String, label: &str, style| {
            let fade = Fade::new(painter, format!("permission-{id}-{key}"));
            theme
                .button(label.to_owned(), style, Some(fade))
                .id(SharedString::from(key.to_owned()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let option_id = option_id.clone();
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.with_session(id, cx, |chat| chat.respond_permission(option_id));
                    });
                }))
        };
        let body = match alert(&prompt.options) {
            // How long the answer holds, then the answers — the affirmative
            // last, where macOS puts the default.
            Some((deny, allow)) => div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                // An agent offering neither *always* form has no second
                // question to ask, and the row is just the two buttons.
                .children((deny.always.is_some() || allow.always.is_some()).then(|| {
                    div()
                        .id("permission-always")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .child(theme.checkbox(prompt.always))
                        .child(
                            div()
                                .text_style(TextStyle::Callout)
                                .text_color(theme.text_muted)
                                .child("Always allow"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace
                                    .with_session(id, cx, |chat| chat.toggle_permission_always());
                            });
                        }))
                }))
                // Whatever is on the left is on the left: the answers hold the
                // trailing edge whether or not the checkbox is there.
                .child(div().flex_1())
                .child(answer(
                    "deny",
                    deny.id(prompt.always),
                    deny.label,
                    ButtonStyle::Ghost,
                ))
                .child(answer(
                    "allow",
                    allow.id(prompt.always),
                    allow.label,
                    ButtonStyle::Prominent,
                ))
                .into_any_element(),
            // Every option the agent sent, one full-width row each. A label of
            // any length reads here, which is the whole point of stacking them.
            None => div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(prompt.options.iter().enumerate().map(|(ix, option)| {
                    let style = match option.kind {
                        PermissionOptionKind::AllowOnce => ButtonStyle::Prominent,
                        _ => ButtonStyle::Ghost,
                    };
                    answer(
                        &format!("option-{ix}"),
                        option.id.clone(),
                        &option.name,
                        style,
                    )
                    .w_full()
                    .justify_center()
                }))
                .into_any_element(),
        };
        Some(
            div()
                .rounded(px(Theme::surface_radius()))
                .px(px(14.))
                .py(px(12.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap(px(8.))
                        .child(
                            icons::icon(icons::security::Key)
                                .size(px(14.))
                                .flex_none()
                                // A glyph's box is its size and the line beside
                                // it is taller, so it drops to meet the text.
                                .mt(px(3.))
                                .text_color(theme.text_muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_style(TextStyle::Body)
                                .text_color(theme.text)
                                .child(prompt.title.clone()),
                        ),
                )
                .child(body)
                .surface(&theme, composer::SURFACE),
        )
    }

    /// A pick from one of the composer's switches — the session's mode, or a
    /// config option like the model.
    pub(crate) fn switch(
        &mut self,
        id: &composer::SwitchId,
        value: &SharedString,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.workspace.read(cx).active_session().map(|chat| chat.id) else {
            return;
        };
        let value = value.to_string();
        self.workspace.update(cx, |workspace, cx| match id {
            composer::SwitchId::Mode => workspace.set_session_mode(session, value, cx),
            composer::SwitchId::Config(config) => workspace.set_session_config(
                session,
                config.to_string(),
                SessionConfigOptionValue::ValueId {
                    value: value.into(),
                },
                cx,
            ),
        });
        cx.notify();
    }

    /// Prompts waiting for the in-flight turn — a steer, drawn as what it is:
    /// the message you have already written, not yet sent. The same bubble the
    /// transcript gives a sent one, held back to the muted tone, and an ✕ to
    /// take it back while it is still yours to take back.
    fn queue(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        if chat.queue.is_empty() {
            return None;
        }
        let id = chat.id;
        Some(div().flex().flex_col().items_end().gap(px(6.)).children(
            chat.queue.iter().enumerate().map(|(ix, text)| {
                // An svg paints in its own `text_color` and inherits none,
                // so the ✕ takes the bubble's group to light with it.
                let group = SharedString::from(format!("steer-{ix}"));
                div()
                    .group(group.clone())
                    .max_w(px(440.))
                    .px(px(14.))
                    .py(px(9.))
                    .rounded(px(Theme::surface_radius()))
                    .bg(theme.surface_raised.opacity(0.6))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_style(TextStyle::Body)
                            .text_color(theme.text_muted)
                            .child(text.clone()),
                    )
                    .child(
                        div()
                            .id(("unqueue", ix))
                            .flex_none()
                            // Onto the first line's baseline, so a steer
                            // that wraps keeps its ✕ at the top.
                            .mt(px(4.))
                            .cursor_pointer()
                            .child(
                                icons::icon(icons::notifications::X)
                                    .size(px(12.))
                                    .text_color(theme.text_faint)
                                    .group_hover(group, |el| el.text_color(theme.text)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.with_session(id, cx, |chat| chat.unqueue(ix));
                                });
                            })),
                    )
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, PermissionOptionKind, alert};

    fn choice(id: &str, kind: PermissionOptionKind) -> Choice {
        Choice {
            id: id.to_owned(),
            name: id.to_owned(),
            kind,
        }
    }

    /// The set every agent sends: two answers, one of them rememberable.
    #[test]
    fn a_yes_a_no_and_a_forever_is_an_alert() {
        let options = vec![
            choice("yes", PermissionOptionKind::AllowOnce),
            choice("yes-always", PermissionOptionKind::AllowAlways),
            choice("no", PermissionOptionKind::RejectOnce),
        ];
        let (deny, allow) = alert(&options).expect("two sides, all three covered");
        assert_eq!(allow.id(false), "yes");
        assert_eq!(allow.id(true), "yes-always");
        // No always-form to send: the refusal stands for this call either way.
        assert_eq!(deny.id(true), "no");
    }

    /// An option neither side accounts for is an option the alert would drop.
    #[test]
    fn a_kind_of_the_agents_own_falls_to_the_stack() {
        let options = vec![
            choice("yes", PermissionOptionKind::AllowOnce),
            choice("no", PermissionOptionKind::RejectOnce),
            choice("edit", PermissionOptionKind::Other("edit_first".into())),
        ];
        assert!(alert(&options).is_none());
    }

    /// So is a second option of a kind one side has already taken.
    #[test]
    fn a_repeated_kind_falls_to_the_stack() {
        let options = vec![
            choice("yes", PermissionOptionKind::AllowOnce),
            choice("yes-too", PermissionOptionKind::AllowOnce),
            choice("no", PermissionOptionKind::RejectOnce),
        ];
        assert!(alert(&options).is_none());
    }

    /// An alert needs both answers — a lone side has nothing to sit opposite.
    #[test]
    fn one_sided_falls_to_the_stack() {
        let options = vec![
            choice("yes", PermissionOptionKind::AllowOnce),
            choice("yes-always", PermissionOptionKind::AllowAlways),
        ];
        assert!(alert(&options).is_none());
    }
}
