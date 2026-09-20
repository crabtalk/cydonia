//! The detail column and its floating plan, permission, and composer controls.

use crate::{
    model::{
        session::{ChatSession, Choice},
        workspace::Showing,
    },
    view::{
        component::{
            composer,
            menu::{self, Menu},
            ribbon, transcript,
        },
        leaf::Pane,
        root::{self, Cydonia, NewSession},
        settings::Section,
    },
};
use artifact::{layout::Member, session::chat::PlanStatus};
use bezel::{
    gpui::{
        AnyElement, App, Axis, Context, Div, DragMoveEvent, Empty, FocusHandle, Focusable as _,
        SharedString, Stateful, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        floating,
        icons::{self, Icon},
        menu::Item,
        popover, surface,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Controls, Status},
    },
};
use cacp::schema::{
    PermissionOptionKind, SessionConfigKind, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOption, SessionConfigSelectOptions, SessionModeState,
};
use std::path::{Path, PathBuf};
use surface::Surfaced as _;

/// Separate from the sidebar's payload so its resize listener stays idle.
struct ChangesResize;

struct TerminalResize;

/// What the chat keeps for itself while the panel stands beside it.
const CHAT_MIN: f32 = 240.;

/// What the panel needs to be worth standing there at all. A diff narrower
/// than this is one nobody can read, so the column is not split below it —
/// see [`panel_beside`].
const PANEL_MIN: f32 = 280.;

/// The widest the panel is given before anybody drags it.
const PANEL_MAX: f32 = 440.;

/// And the share of the column it takes between the two.
const PANEL_SHARE: f32 = 0.33;

/// Whether there is room to stand the panel beside the chat.
///
/// Below this the panel covers the column instead — see [`Cydonia::detail`].
/// Squeezing both is the answer neither of them wants: the platform's own
/// split view collapses a sidebar at a minimum thickness rather than thinning
/// it past use, and a 200px diff is past use.
pub fn panel_beside(available: f32) -> bool {
    available >= CHAT_MIN + PANEL_MIN
}

/// How wide the panel is drawn.
///
/// `preferred` is `None` until somebody drags the split. A width nobody chose
/// is a share of what there is, bounded at both ends, so the same build is not
/// giving a third of a laptop screen to the same slab it gives a sixth of a
/// display. A width somebody did choose is kept as far as it fits: the chat
/// keeps [`CHAT_MIN`], and the panel never takes over half the column, so a
/// width dragged on a display is not the whole of a laptop window.
pub fn panel_width(preferred: Option<f32>, available: f32) -> f32 {
    let preferred =
        preferred.unwrap_or_else(|| (available * PANEL_SHARE).clamp(PANEL_MIN, PANEL_MAX));
    let min = PANEL_MIN.min(available / 2.);
    let max = (available - CHAT_MIN).min(available / 2.).max(min);
    preferred.clamp(min, max)
}

fn panel_height(preferred: f32, available: f32) -> f32 {
    let min = 120.0_f32.min(available / 2.);
    preferred.clamp(min, (available - 160.).max(min))
}

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
pub struct Verdict<'a> {
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
    pub fn id(&self, always: bool) -> String {
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
pub fn alert(options: &[Choice]) -> Option<(Verdict<'_>, Verdict<'_>)> {
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

/// What is left to do where there is no agent to reach — the line under
/// [`agent_missing`]. `agent` is the one the session was opened on, and
/// nothing where a session was asked for and never opened; `others` is whether
/// `settings.toml` names any agent at all.
///
/// Three states with three ways out, and the shortest is worth saying. A
/// session whose own agent has gone while another is still here is the only
/// thing stranded, and opening one on that agent beats a download. Alone, it
/// is the install. Asked for on a machine with nothing on it, there is no
/// session yet to be stranded and the install is the whole of it.
pub fn adrift(agent: Option<&str>, others: bool) -> &'static str {
    match (agent, others) {
        (Some(_), true) => "Open a session on an agent that is, or install this one again.",
        (Some(_), false) => "No other agent is installed either.",
        (None, _) => "Install one to open a session here.",
    }
}

/// The title over it: which agent is missing, or that none is here at all — a
/// session asked for on a machine with nothing installed has no agent to name.
pub fn agent_missing(agent: Option<&str>) -> String {
    match agent {
        Some(agent) => format!("{agent} is not installed"),
        None => "No agent installed".to_owned(),
    }
}

/// The same said in one line, for the strip that stands under a transcript.
///
/// It names the place, where [`adrift`] does not: the empty state has a button
/// under it and the strip has only itself, so the destination has to be in the
/// words rather than on a control beneath them.
pub fn adrift_line(agent: &str, others: bool) -> String {
    let missing = agent_missing(Some(agent));
    match others {
        true => format!(
            "{missing} — open a session on an agent that is, or install it again in Settings › Agents."
        ),
        false => format!("{missing} — install one in Settings › Agents."),
    }
}

impl Cydonia {
    /// Where the window's shell opens: under a layout, the project the first
    /// pane is in; the session's working directory when a chat is in front —
    /// its worktree, where it has one — and the project's otherwise.
    ///
    /// Asked again for each tab, not once for the panel: the panel outlives
    /// whatever was in front when it was opened.
    pub(crate) fn shell_cwd(&self, cx: &App) -> Option<PathBuf> {
        let workspace = self.workspace.read(cx);
        if let Some(layout) = workspace.active_layout() {
            return layout.panes().into_iter().next().map(|pane| pane.project);
        }
        if self.showing(cx) == Some(Pane::Chat)
            && let Some(chat) = workspace.active_session()
        {
            return Some(chat.cwd.clone());
        }
        Some(workspace.active_project()?.path.clone())
    }

    pub(crate) fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal.is_none() {
            let Some(cwd) = self.shell_cwd(cx) else {
                return;
            };
            let this = cx.weak_entity();
            let panel = cx.new(|cx| {
                super::component::terminal::TerminalPanel::new(
                    &cwd,
                    move |cx| this.upgrade()?.read(cx).shell_cwd(cx),
                    window,
                    cx,
                )
            });
            // The last tab closing takes the panel with it: an empty bottom
            // panel is a band of nothing with a `+` in it.
            cx.subscribe_in(
                &panel,
                window,
                |this, _, _: &super::component::terminal::Empty, window, cx| {
                    if this.terminal.take().is_some_and(|(visible, _)| visible) {
                        this.focus_after_terminal(window, cx);
                    }
                    cx.notify();
                },
            )
            .detach();
            self.terminal = Some((false, panel));
        }
        let Some((visible, panel)) = self.terminal.as_mut() else {
            return;
        };
        *visible = true;
        let panel = panel.clone();
        window.focus(&panel.focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn toggle_terminal(
        &mut self,
        _: &root::ToggleTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((visible, _)) = self.terminal.as_mut()
            && *visible
        {
            *visible = false;
            self.focus_after_terminal(window, cx);
            cx.notify();
        } else {
            self.show_terminal(window, cx);
        }
    }

    /// Where the focus lands when the panel goes down. The composer is the
    /// chat's, and a pane without one takes the window's own handle — leaving
    /// it on the shut panel is leaving it nowhere.
    fn focus_after_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.showing(cx) == Some(Pane::Chat) {
            true => window.focus(&self.composer_focus_handle(cx), cx),
            false => window.focus(&self.focus, cx),
        }
    }

    pub fn composer_focus_handle(&self, cx: &App) -> FocusHandle {
        self.leaf().composer.focus_handle(cx)
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

    pub(crate) fn submit(
        &mut self,
        text: String,
        attachments: Vec<crate::model::media::Attachment>,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.workspace.read(cx).active_id() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| match attachments.is_empty() {
                true => workspace.send(id, text, cx),
                false => workspace.send_attached(id, text, &attachments, cx),
            });
        // On the newest line, whatever was being read a moment ago — see
        // [`transcript::State::follow_tail`].
        if let Some(chat) = self.workspace.read(cx).session(id) {
            chat.transcript.follow_tail();
        }
    }

    /// The bar over a run picked out of the transcript: what to do with it,
    /// where it was picked.
    ///
    /// The article's ribbon in a second place — same perch, same layer, and
    /// the same rule that a bar belongs to a run rather than to a pointer, so
    /// it arrives when the drag ends and goes when the run does. Two words
    /// rather than glyphs: there is room for them, and neither has a mark
    /// anybody reads without being told.
    fn selection_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (view, head) = self
            .workspace
            .read(cx)
            .active_session()?
            .transcript
            .selection_perch()?;
        let at = ribbon::perch(view, head)?;
        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .child(
                self.selection_action("Copy", icons::text::Copy, &theme, cx, |this, _, cx| {
                    this.workspace.update(cx, |workspace, cx| {
                        workspace.copy_selection(cx);
                        workspace.clear_selection(cx);
                    });
                }),
            )
            .child(self.selection_action(
                "Quote",
                icons::text::TextQuote,
                &theme,
                cx,
                |this, window, cx| this.quote_selection(window, cx),
            ));
        Some(ribbon::floated(
            "transcript-selection",
            at,
            card.into_any_element(),
        ))
    }

    /// One of the bar's two, as a word beside its mark.
    fn selection_action(
        &self,
        label: &'static str,
        glyph: &'static [u8],
        theme: &Theme,
        cx: &Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        theme
            .ghost(SharedString::from(format!("selection-{label}")))
            .px(px(8.))
            .py(px(4.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            .child(
                icons::icon(glyph)
                    .size(px(13.))
                    .flex_none()
                    .text_color(theme.text_muted),
            )
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| act(this, window, cx)))
            .into_any_element()
    }

    /// Answer the run: the composer takes it, and the caret goes where the
    /// reply is written. The run is dropped with it — the bar has done what it
    /// was for, and a highlight with nothing over it reads as half a state.
    fn quote_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .workspace
            .read(cx)
            .active_session()
            .and_then(|chat| chat.transcript.copied(chat));
        let Some(text) = text else {
            return;
        };
        self.leaf()
            .composer
            .update(cx, |composer, cx| composer.quote(text, cx));
        self.workspace
            .update(cx, |workspace, cx| workspace.clear_selection(cx));
        window.focus(&self.composer_focus_handle(cx), cx);
    }

    pub(crate) fn cancel_turn(&mut self, cx: &mut Context<Self>) {
        self.with_active(cx, |chat| chat.cancel());
    }

    /// The composer's agent chip. An ACP session is bound to the process that
    /// serves it, so picking another agent opens a session rather than
    /// swapping one out from under a transcript.
    pub(crate) fn pick_agent(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let entry = self.workspace.read(cx).settings.agents.get(ix).cloned();
        if let Some(entry) = entry {
            // A session that does not exist yet is in no arrangement — it has
            // no file, so a layout has nothing to name it by. `None` is that
            // said plainly, and leaving the layout is what puts the window
            // where the new session is about to be.
            self.enter_member(None, window, cx);
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
        let arranged = workspace.active_layout().is_some();
        let agents: Vec<composer::Agent> = workspace
            .settings
            .agents
            .iter()
            .map(|entry| composer::Agent {
                name: entry.name.clone().into(),
                icon: workspace.agent_icon(&entry.name),
            })
            .collect();
        // The session each pane is on: its own where a layout put it there,
        // and whatever the project is on for the single pane.
        let on: Vec<Option<u64>> = self
            .leaves
            .iter()
            .map(|leaf| match &leaf.entry {
                Some(entry) => workspace
                    .showing_of(entry)
                    .and_then(|(_, showing)| match showing {
                        Showing::Session(id) => Some(id),
                        _ => None,
                    }),
                None => workspace.active_id(),
            })
            .collect();
        // Read out before writing: the composers are updated through `cx`,
        // which the workspace is borrowed from.
        let mut pointed = Vec::with_capacity(on.len());
        for id in on {
            let chat = id.and_then(|id| workspace.session(id));
            let placeholder = chat.map_or_else(
                || "message the agent…".to_owned(),
                |chat| format!("message {}…", chat.entry.name),
            );
            let current = chat
                .map(|chat| chat.entry.name.clone())
                .and_then(|name| agents.iter().position(|agent| agent.name == name));
            // Both belong to the agent process rather than to the transcript,
            // so a session read back off disk offers neither until it
            // reconnects.
            let live = chat.filter(|chat| chat.live());
            pointed.push((
                chat.map(|chat| chat.id),
                chat.map(|chat| chat.draft.clone()).unwrap_or_default(),
                placeholder,
                chat.map(|chat| chat.commands.clone()).unwrap_or_default(),
                chat.is_some_and(|chat| chat.streaming),
                chat.and_then(composer::Activity::of),
                current,
                live.map(switches).unwrap_or_default(),
                live.and_then(|chat| chat.usage),
            ));
        }
        for (leaf, point) in self.leaves.iter().zip(pointed) {
            let (session, draft, placeholder, commands, streaming, activity, current, sw, usage) =
                point;
            leaf.composer.update(cx, |composer, cx| {
                composer.set_tools(!arranged, cx);
                composer.set_session(session, &draft, cx);
                composer.set_placeholder(&placeholder, cx);
                composer.set_commands(&commands, cx);
                composer.set_streaming(streaming, cx);
                composer.set_activity(activity, cx);
                composer.set_agents(&agents, current, cx);
                composer.set_switches(&sw, cx);
                composer.set_usage(usage, cx);
            });
        }
    }

    pub(crate) fn detail(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        // Nothing to send to: no session at all, or one whose agent has gone
        // from settings.toml, leaving nothing to reconnect it to.
        let live = self.workspace.read(cx).reachable();
        let showing = self.showing(cx);
        let arranged = self.workspace.read(cx).active_layout().is_some();
        // A layout arranges several entries, so it draws its own panes. One
        // entry open on its own is the single pane below.
        let body = match self.panes(window, cx) {
            Some(panes) => panes,
            None => match showing {
                None => self.launch(cx),
                Some(Pane::Chat) => {
                    self.conversation(self.workspace.read(cx).active_id(), None, window, cx)
                }
                Some(Pane::Board) => match self.workspace.read(cx).active.zip(
                    self.workspace
                        .read(cx)
                        .active_project()
                        .and_then(|open| open.board),
                ) {
                    Some((project, at)) => self.board(project, at, None, window, cx),
                    None => self.launch(cx),
                },
                // An entry can be named and not yet loaded — an article holds
                // no editor until it is opened. The front door stands in for
                // the moment in between.
                Some(Pane::Article) => self
                    .workspace
                    .read(cx)
                    .active
                    .zip(
                        self.workspace
                            .read(cx)
                            .active_project()
                            .and_then(|open| open.article),
                    )
                    .and_then(|(project, at)| self.article(project, at, None, window, cx))
                    .unwrap_or_else(|| self.launch(cx)),
                Some(Pane::Table) => self
                    .workspace
                    .read(cx)
                    .active
                    .zip(
                        self.workspace
                            .read(cx)
                            .active_project()
                            .and_then(|open| open.table),
                    )
                    .and_then(|(project, at)| self.table(project, at, None, cx))
                    .unwrap_or_else(|| self.launch(cx)),
            },
        };

        let body = match arranged {
            true => body,
            // One pane takes a drop on its edge too: that is where the first
            // layout comes from.
            false => self.lone_pane(body, cx),
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
            //
            // A layout takes none of it: its panes carry a bar each, and the
            // one at the top left keeps clear of the lights itself.
            .when(!arranged, |el| el.pt(px(root::HEADER_HEIGHT)))
            .child(body);

        let footer_height = self
            .workspace
            .read(cx)
            .active_session()
            .map(|chat| chat.transcript.footer_height.clone());
        let main = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .bg(root::content_bg(&theme))
            .flex()
            .flex_col()
            .child(content)
            // After the content, so it draws over it. A layout has no band of
            // its own: one title over several panes would name whichever is in
            // front and say nothing about the rest.
            .children((!arranged).then(|| self.pane_header(window, cx)))
            .children(match showing == Some(Pane::Chat) {
                true => self.selection_bar(cx),
                false => None,
            })
            // Scroll content beneath the glass; bottom padding clears the last message.
            //
            // A chat with nowhere to send stands the reason there in its place
            // — the slot is what the eye goes to for what happens next, and a
            // composer simply withheld leaves it answering nothing.
            .when(
                showing == Some(Pane::Chat) && !arranged,
                |column| match live {
                    // The whole pane takes a dropped picture for the composer.
                    true => column
                        .on_drop(
                            cx.listener(|this, paths: &bezel::gpui::ExternalPaths, _, cx| {
                                this.leaf()
                                    .composer
                                    .update(cx, |composer, cx| composer.drop_paths(paths, cx));
                            }),
                        )
                        .child(footer(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .children(self.plan(cx))
                                .children(self.permission(cx))
                                .child(self.leaf().composer.clone()),
                            footer_height.clone(),
                        )),
                    false => column.children(
                        self.adrift_strip(cx)
                            .map(|strip| footer(strip, footer_height.clone())),
                    ),
                },
            );
        // The one panel the window has, under whatever is showing: a layout's
        // panes included, which is what the right panel cannot do.
        let terminal = self
            .terminal
            .as_ref()
            .filter(|(visible, _)| *visible)
            .map(|(_, terminal)| terminal.clone());
        // The window's own panels stand beside one entry, not beside an
        // arrangement of several. Held rather than shut, so leaving the layout
        // puts them back as they were.
        let changes = match arranged {
            true => None,
            false => self.changes.clone(),
        };
        let available = f32::from(window.viewport_size().width)
            - if self.sidebar_open {
                self.sidebar_width
            } else {
                0.
            };
        let available = available.max(0.);
        // Beside the chat, or over it in a window too narrow to hold both.
        let beside = panel_beside(available);
        let width = panel_width(self.changes_width, available);
        let height = panel_height(
            self.terminal_height,
            f32::from(window.viewport_size().height),
        );
        div()
            .id("session-panels")
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<TerminalResize>, _, cx| {
                    this.terminal_height = panel_height(
                        f32::from(event.bounds.bottom() - event.event.position.y),
                        f32::from(event.bounds.size.height),
                    );
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("session-detail")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<ChangesResize>, _, cx| {
                            this.changes_width = Some(panel_width(
                                Some(f32::from(event.bounds.right() - event.event.position.x)),
                                f32::from(event.bounds.size.width),
                            ));
                            this.save_panel_layout_settled(cx);
                            cx.notify();
                        },
                    ))
                    .child(main)
                    .children(changes.clone().filter(|_| beside).map(|panel| {
                        div()
                            .relative()
                            .w(px(width))
                            .min_w_0()
                            .flex_none()
                            .child(panel)
                    }))
                    // Over the chat rather than beside it. The chat stays in
                    // the tree behind it, so what it was scrolled to and what
                    // was typed into it are still there when the window is
                    // widened again — and the panel paints its own background,
                    // so nothing reads through.
                    .children(
                        changes
                            .clone()
                            .filter(|_| !beside)
                            .map(|panel| div().absolute().inset_0().child(panel)),
                    )
                    // No split to drag when there is nothing beside anything.
                    .when(changes.is_some() && beside, |row| {
                        row.child(
                            crate::view::component::divider::divider(&theme, Axis::Horizontal)
                                .id("changes-split")
                                .absolute()
                                .top_0()
                                .right(px(width - crate::view::component::divider::HIT / 2.))
                                .on_drag(ChangesResize, |_, _, _, cx| cx.new(|_| Empty)),
                        )
                    }),
            )
            .children(
                terminal
                    .clone()
                    .map(|terminal| div().h(px(height)).flex_none().child(terminal)),
            )
            .when(terminal.is_some(), |column| {
                column.child(
                    crate::view::component::divider::divider(&theme, Axis::Vertical)
                        .id("terminal-split")
                        .absolute()
                        .left_0()
                        .bottom(px(height - crate::view::component::divider::HIT / 2.))
                        .on_drag(TerminalResize, |_, _, _, cx| cx.new(|_| Empty)),
                )
            })
    }
}

/// Where the composer floats, and where anything standing in for it goes: out
/// of flow at the column's foot, held to the composer's own width so the two
/// land on the same edges.
pub(crate) fn footer(
    inner: impl IntoElement,
    height: Option<std::rc::Rc<std::cell::Cell<bezel::gpui::Pixels>>>,
) -> impl IntoElement {
    div()
        .absolute()
        .bottom(px(root::COMPOSER_BOTTOM))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            // A layer, not a box: the band floats over the stream, and what it
            // covers is its own — see [`floating::layer`].
            floating::layer("composer-band")
                .w_full()
                .max_w(px(root::COMPOSER_COLUMN))
                .px(px(root::COMPOSER_MARGIN))
                .relative()
                .child(inner)
                .child(
                    bezel::gpui::canvas(
                        move |bounds, window, _| {
                            if let Some(height) = &height
                                && (height.replace(bounds.size.height) - bounds.size.height).abs()
                                    > px(0.5)
                            {
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
        )
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
    pub(crate) fn launch(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let agents: Vec<(String, Option<Icon>)> = workspace
            .settings
            .agents
            .iter()
            .map(|entry| (entry.name.clone(), workspace.agent_icon(&entry.name)))
            .collect();
        let mut rows: Vec<AnyElement> = Vec::new();
        // One row, and a panel of agents under it once there is a choice to
        // make: a bare "New session" opens on whichever agent is first, and
        // nothing on this screen would say which.
        if sessions && agents.len() > 1 {
            let picks: Vec<_> = agents
                .into_iter()
                .enumerate()
                .map(|(at, (name, icon))| {
                    let icon = icon.unwrap_or_else(|| icons::social::MessageCircle.into());
                    menu::row(
                        Item::action(name).with_icon(icon),
                        move |this, window, cx| this.pick_agent(at, window, cx),
                    )
                })
                .collect();
            let trigger = self.make_row(
                "session",
                "New session",
                icons::social::MessageCirclePlus,
                cx,
                move |this, _, cx| this.toggle_menu(Menu::Launch, cx),
            );
            rows.push(
                self.menu_press(trigger, Menu::Launch, cx)
                    .relative()
                    .children((self.menu == Some(Menu::Launch)).then(|| {
                        popover::anchored_menu_below(
                            "launch-menu",
                            self.menu_card("launch-menu", picks, cx),
                            None,
                        )
                    }))
                    .into_any_element(),
            );
        } else if sessions {
            rows.push(
                self.make_row(
                    "session",
                    "New session",
                    icons::social::MessageCirclePlus,
                    cx,
                    move |this, window, cx| this.new_session_action(&NewSession, window, cx),
                )
                .into_any_element(),
            );
        }
        if boards {
            rows.push(
                self.make_row(
                    "board",
                    "New board",
                    icons::development::SquareKanban,
                    cx,
                    move |this, window, cx| this.ask_new_board(ix, window, cx),
                )
                .into_any_element(),
            );
        }
        rows.push(
            self.make_row(
                "article",
                "New article",
                icons::files::FilePlus,
                cx,
                move |this, window, cx| this.new_article(ix, window, cx),
            )
            .into_any_element(),
        );
        if tables {
            rows.push(
                self.make_row(
                    "table",
                    "New table",
                    icons::files::Table2,
                    cx,
                    move |this, window, cx| this.new_table(ix, window, cx),
                )
                .into_any_element(),
            );
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
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        glyph: impl Into<Icon>,
        cx: &mut Context<Self>,
        make: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let (id, label) = (id.into(), label.into());
        // An svg paints in its own `text_color` and inherits none, so the glyph
        // cannot ride the row's hover. Both halves take the row's group instead,
        // which lights them together — the group is named per row so hovering
        // one does not light the rest.
        div()
            .id(id.clone())
            .group(id.clone())
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
                    .group_hover(id.clone(), |el| el.text_color(theme.text)),
            )
            .child(
                div()
                    .text_color(theme.text_muted)
                    .group_hover(id, |el| el.text_color(theme.text))
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, window, cx| make(this, window, cx)))
    }

    /// The session a chat pane is on. Nothing where one was asked for with no
    /// agent to open it on — see [`Cydonia::asked_session`].
    pub(crate) fn conversation(
        &self,
        on: Option<u64>,
        entry: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(chat) = on.and_then(|on| workspace.session(on)) else {
            // No session, and the pane showing regardless: one was asked for
            // with no agent to open it on — see [`Cydonia::asked_session`].
            return match self.leaf_of(entry).asked_session {
                true => self.no_agent(None, false, cx),
                false => div().flex_1().into_any_element(),
            };
        };
        // Nothing has been said yet, so what the session has to show for
        // itself is the directory the agent was started in.
        if chat.unsaid() && chat.fork.is_none() && chat.queue.is_empty() {
            let agent = chat.entry.name.clone();
            let cwd = workspace
                .active_project()
                .map(|project| shown_path(&project.path))
                .unwrap_or_default();
            // Except where the agent is not here to be started in it. A
            // transcript that has nothing to show *and* nowhere to send is the
            // whole pane, so the way out goes here rather than under it.
            let others = !workspace.settings.agents.is_empty();
            if !workspace.reachable() {
                return self.no_agent(Some(&agent), others, cx);
            }
            return theme
                .empty_state(icons::files::Folder, cwd, format!("{agent} runs here"))
                .flex_1()
                .into_any_element();
        }
        let id = chat.id;
        let available = (f32::from(window.viewport_size().width)
            - if self.sidebar_open {
                self.sidebar_width
            } else {
                0.
            })
        .max(0.);
        // The right-hand panel is not drawn beside a layout — see
        // [`Cydonia::detail`] — so its width is only taken off the column
        // where it is actually standing there.
        // A panel covering the column takes none of it away — the chat is
        // still laid out at full width underneath.
        let beside = self.changes.is_some()
            && self.workspace.read(cx).active_layout().is_none()
            && panel_beside(available);
        let column = available
            - match beside {
                true => panel_width(self.changes_width, available),
                false => 0.,
            };
        // The column, less what a layout gives the panes beside this one. The
        // transcript sizes its margins off this and drops the rail when they
        // are too narrow to hold it — measured against the window, a pane in a
        // split would keep a rail there is no room for and draw it over the
        // prose.
        let pane_width = column * self.width_share(entry, cx);
        let root = cx.entity().downgrade();
        let queued = move |window: &mut Window, cx: &mut bezel::gpui::App| {
            root.update(cx, |root, cx| {
                root.queue(window, cx).map(IntoElement::into_any_element)
            })
            .ok()
            .flatten()
        };
        self.workspace
            .update(cx, |workspace, cx| match workspace.session(id) {
                Some(chat) => transcript::render(chat, pane_width, queued, window, cx),
                None => div().flex_1().into_any_element(),
            })
    }

    /// No agent to say anything to: what is missing, and the way to put it
    /// there. The whole pane, because there is nothing else in it.
    ///
    /// Two ways in. A session with nothing said in it whose agent has gone —
    /// uninstalled while it was still on the rail — which before this drew
    /// blank: a transcript with no messages, under a composer withheld for
    /// having nowhere to send, which together are nothing at all. And a
    /// session asked for on a machine with no agent on it, which has no
    /// transcript of its own to stand over and names none.
    fn no_agent(&self, agent: Option<&str>, others: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        theme
            .empty_state(
                icons::files::Download,
                agent_missing(agent),
                adrift(agent, others),
            )
            .flex_1()
            .child(
                theme
                    .button(
                        "Install an agent…",
                        ButtonStyle::Prominent,
                        Some(Fade::new(painter, "install-agent-empty")),
                    )
                    .id("install-agent-empty")
                    .on_click(
                        cx.listener(|this, _, _, cx| this.open_settings(Section::Agents, cx)),
                    ),
            )
            .into_any_element()
    }

    /// What stands where the composer would be over a transcript there is no
    /// longer anywhere to answer into. The strip is the click: it names where
    /// the agent is installed, and going there is all it has to offer.
    ///
    /// Nothing at all where the session has said nothing — [`Self::no_agent`]
    /// is the whole pane then, and this under it would say it twice.
    fn adrift_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let chat = workspace.active_session().filter(|chat| !chat.unsaid())?;
        let message = adrift_line(&chat.entry.name, !workspace.settings.agents.is_empty());
        Some(
            theme
                .warning_strip(message)
                .id("adrift")
                .mt(px(0.))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.open_settings(Section::Agents, cx)))
                .into_any_element(),
        )
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

    fn take_queued(
        &mut self,
        id: u64,
        ix: usize,
        expected: &str,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let mut text = None;
        self.workspace.update(cx, |workspace, cx| {
            if workspace.active_id() != Some(id) {
                return;
            }
            workspace.with_session(id, cx, |chat| {
                if chat.queue.get(ix).is_some_and(|queued| queued == expected) {
                    text = chat.queue.remove(ix);
                }
            });
        });
        text
    }

    /// Prompts waiting for the current turn, with edit and cancel actions.
    fn queue(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).active_session()?;
        let id = chat.id;
        let cwd = chat.cwd.clone();
        let queue = chat.queue.clone();
        self.leaf_mut()
            .queued_galleries
            .retain(|(session, ix, text), _| *session == id && queue.get(*ix) == Some(text));
        if queue.is_empty() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .children(queue.iter().enumerate().map(|(ix, text)| {
                    let edit_text = text.clone();
                    let cancel_text = text.clone();
                    let (doc, images) = transcript::gallery::document(text);
                    let gallery = (!images.is_empty()).then(|| {
                        self.leaf_mut()
                            .queued_galleries
                            .entry((id, ix, text.clone()))
                            .or_insert_with(|| {
                                cx.new(|cx| transcript::gallery::Gallery::new(images, &cwd, cx))
                            })
                            .clone()
                    });
                    div()
                        .max_w(px(440.))
                        .when(gallery.is_some(), |row| row.w(px(440.)).max_w_full())
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(4.))
                        .child(
                            div()
                                .min_w_0()
                                .when(gallery.is_some(), |bubble| bubble.w_full())
                                .px(px(14.))
                                .py(px(9.))
                                .rounded(px(Theme::surface_radius()))
                                .bg(theme.surface_raised.opacity(0.6))
                                .text_style(TextStyle::Body)
                                .text_color(theme.text_muted)
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .when(!doc.blocks.is_empty(), |bubble| {
                                    bubble.child(markdown::render(
                                        &doc,
                                        Default::default(),
                                        window,
                                        cx,
                                    ))
                                })
                                .children(gallery),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .gap(px(2.))
                                .text_style(TextStyle::Caption)
                                .text_color(theme.text_muted)
                                .child(
                                    theme
                                        .ghost(("edit-queued", ix))
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            icons::icon(icons::text::Pencil)
                                                .size(px(12.))
                                                .text_color(theme.text_muted),
                                        )
                                        .tooltip(|window, cx| {
                                            Tooltip::text("Edit queued message", window, cx)
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if let Some(text) =
                                                this.take_queued(id, ix, &edit_text, cx)
                                            {
                                                this.leaf().composer.update(cx, |composer, cx| {
                                                    composer.restore_queued(text, window, cx);
                                                });
                                            }
                                        })),
                                )
                                .child(
                                    theme
                                        .ghost(("cancel-queued", ix))
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(
                                            icons::icon(icons::notifications::X)
                                                .size(px(12.))
                                                .text_color(theme.text_muted),
                                        )
                                        .tooltip(|window, cx| {
                                            Tooltip::text("Cancel queued message", window, cx)
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.take_queued(id, ix, &cancel_text, cx);
                                        })),
                                ),
                        )
                })),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/queued_images.rs"]
mod queued_image_tests;
