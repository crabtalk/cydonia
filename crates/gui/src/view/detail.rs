//! The detail column and its floating plan, permission, and composer controls.

use crate::{
    model::{
        session::{ChatSession, Choice, Command, Usage},
        session_preferences::{self, Offers},
        workspace::{ArticleChange, Opening, Showing, Workspace},
    },
    view::{
        component::{composer, transcript},
        kit::{
            menu::{self, Menu, MenuHost as _},
            new::{self, New},
            text::ShownPath as _,
        },
        window::{
            chrome,
            root::{self, Cydonia, NewArticle, NewBoard, NewSession, NewTable},
        },
    },
};
use artifact::{kind::Kind, session::chat::ChatItem, space::Member};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Context, Div, DragMoveEvent, Empty, FocusHandle,
        Focusable as _, Pixels, Point, SharedString, Stateful, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        context_menu::Place,
        floating,
        icons::{self, Icon},
        keys,
        menu::Item,
        popover,
        widgets::Content,
    },
};
use cydonia_acp::proto::{
    PermissionOptionKind, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionConfigSelectOptions, SessionModeState,
};
use std::path::{Path, PathBuf};

/// What the chat keeps for itself while the panel stands beside it.
const CHAT_MIN: f32 = 240.;

/// What the panel needs to be worth standing there at all. A diff narrower
/// than this is one nobody can read, so the column is not split below it —
/// see [`panel_beside`].
const PANEL_MIN: f32 = 280.;

/// The share of the column the panel takes before anybody drags it.
const PANEL_SHARE: f32 = 0.5;

/// The bottom panel's height before anybody drags it.
const TERMINAL_HEIGHT: f32 = 240.;

/// Separate from the sidebar's payload so its resize listener stays idle.
struct ChangesResize;

struct TerminalResize;

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
/// `share` is the part of `available` the split was dragged to, and `None`
/// until somebody drags it, which is [`PANEL_SHARE`]. Either is kept as far as
/// the chat keeps [`CHAT_MIN`] and the panel [`PANEL_MIN`].
pub fn panel_width(share: Option<f32>, available: f32) -> f32 {
    let preferred = available * share.unwrap_or(PANEL_SHARE);
    let min = PANEL_MIN.min(available / 2.);
    let max = (available - CHAT_MIN).max(min);
    preferred.clamp(min, max)
}

fn panel_height(preferred: Option<f32>, available: f32) -> f32 {
    let preferred = preferred.unwrap_or(TERMINAL_HEIGHT);
    let min = 120.0_f32.min(available / 2.);
    preferred.clamp(min, (available - 160.).max(min))
}

/// What `offers` can be switched between, flattened to the one shape the
/// composer draws: the config options, then the modes. Every composer's agent
/// menu is built here.
///
/// Config first, because the model is a config option and it is the one people
/// came for. Booleans are left out — a menu of two rows is a toggle wearing a
/// menu, and the agent menu has nowhere to put a real one yet.
fn switches_of(offers: &Offers) -> Vec<composer::Switch> {
    let (modes, config) = (offers.modes.as_ref(), offers.config.as_slice());
    let mut switches: Vec<composer::Switch> = config
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
    if let Some(modes) = modes
        .filter(|modes| !modes.available_modes.is_empty())
        .filter(|modes| !covered_by_config(config, modes))
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
fn covered_by_config(config: &[SessionConfigOption], modes: &SessionModeState) -> bool {
    config.iter().any(|option| {
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
    session_preferences::select_values(options)
        .into_iter()
        .map(|option| composer::SwitchOption {
            id: option.value.to_string().into(),
            name: option.name.clone().into(),
        })
        .collect()
}

/// One of the two answers a permission request comes down to, with the *once*
/// and *always* forms of it read as one.
pub struct Verdict<'a> {
    /// What the button says: the once-form's own label, whatever the checkbox
    /// is set to. Agents word the always-form as a sentence — "Yes, and allow
    /// access to repos/ and ls commands" — and a sentence is not a button.
    pub(crate) label: &'a str,
    once: &'a str,
    pub(crate) always: Option<&'a str>,
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

impl Cydonia {
    /// Where the window's shell opens: under a space, the project the first
    /// pane is in; the session's working directory when a chat is in front —
    /// its worktree, where it has one — and the project's otherwise.
    ///
    /// Asked again for each tab, not once for the panel: the panel outlives
    /// whatever was in front when it was opened.
    pub(crate) fn shell_cwd(&self, cx: &App) -> Option<PathBuf> {
        let workspace = self.workspace.read(cx);
        if let Some(space) = workspace.active_space() {
            return space
                .entries()
                .into_iter()
                .find_map(|pane| pane.project().map(Path::to_path_buf));
        }
        if self.showing(cx) == Some(Kind::Session)
            && let Some(chat) = workspace.active_session(cx)
        {
            return Some(chat.cwd.clone());
        }
        Some(workspace.active_project()?.path.clone())
    }

    /// Nothing without the `desktop` feature: there is no shell to run.
    #[cfg(not(feature = "desktop"))]
    pub(crate) fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.desktop_only("The terminal", window, cx);
    }

    #[cfg(not(feature = "desktop"))]
    pub(crate) fn toggle_terminal(
        &mut self,
        _: &root::ToggleTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_terminal(window, cx);
    }

    #[cfg(feature = "desktop")]
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

    #[cfg(feature = "desktop")]
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
    #[cfg(feature = "desktop")]
    fn focus_after_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self
            .composer_focus_handle(cx)
            .filter(|_| self.showing(cx) == Some(Kind::Session));
        window.focus(composer.as_ref().unwrap_or(&self.focus), cx);
    }

    pub fn composer_focus_handle(&self, cx: &App) -> Option<FocusHandle> {
        Some(self.leaf()?.read(cx).composer.focus_handle(cx))
    }

    fn submit(
        &mut self,
        id: u64,
        text: String,
        attachments: Vec<crate::model::media::Attachment>,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| match attachments.is_empty() {
                true => workspace.send(id, text, cx),
                false => workspace.send_attached(id, text, &attachments, cx),
            });
        // On the newest line, whatever was being read a moment ago — see
        // [`transcript::State::follow_tail`].
        for view in self.session_views(id, cx) {
            view.read(cx).state.follow_tail();
        }
    }

    /// What a press in a composer pointed at `session` asks for — the pane's
    /// and an article card's alike.
    pub(crate) fn composer_event(
        &mut self,
        session: Option<u64>,
        event: &composer::ComposerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use composer::ComposerEvent;
        match event {
            ComposerEvent::Draft(id, draft) => {
                self.workspace.update(cx, |workspace, cx| {
                    workspace.set_draft(*id, draft.clone(), cx)
                });
            }
            ComposerEvent::Terminal => self.show_terminal(window, cx),
            ComposerEvent::Changes => self.show_changes(window, cx),
            ComposerEvent::Files => self.show_files(window, cx),
            _ => {
                let Some(id) = session else {
                    return;
                };
                match event {
                    ComposerEvent::Submit(text, attachments) => {
                        self.submit(id, text.clone(), attachments.clone(), cx)
                    }
                    ComposerEvent::Cancel => self.workspace.update(cx, |workspace, cx| {
                        workspace.with_session(id, cx, |chat| chat.cancel())
                    }),
                    ComposerEvent::Reconnect => self
                        .workspace
                        .update(cx, |workspace, cx| workspace.wake_session(id, cx)),
                    ComposerEvent::Switch(switch, value) => self.switch(id, switch, value, cx),
                    _ => {}
                }
            }
        }
    }

    /// The composer's agent chip. An ACP session is bound to the process that
    /// serves it, so picking another agent opens a session rather than
    /// swapping one out from under a transcript.
    pub(crate) fn pick_agent(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let entry = self.workspace.read(cx).settings.agents.get(ix).cloned();
        let Some(entry) = entry else {
            return;
        };
        let id = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_session(entry, None, cx));
        if let Some(id) = id {
            self.select_session(id, Opening::Kept, window, cx);
        }
    }

    /// What the composer needs from the session it is pointed at: the agent's
    /// name, its commands, whether a turn is in flight, and the agents it can
    /// be swapped for.
    pub(crate) fn sync_composer(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let arranged = workspace.active_space().is_some();
        let agents = composer_agents(workspace);
        // The agent an article's first send opens its session on.
        let first = workspace.article_agent(cx).and_then(|entry| {
            agents
                .iter()
                .position(|agent| agent.name.as_ref() == entry.name)
        });
        let articles: Vec<Option<PathBuf>> = self
            .leaves
            .iter()
            .map(|pane| pane.read(cx).thread_article(cx))
            .collect();
        // The session each tab is on. A tab showing an article is on the
        // article's.
        let pointed: Vec<Pointing> = self
            .leaves
            .iter()
            .zip(&articles)
            .map(|(pane, article)| {
                if let Some(path) = article {
                    let id = workspace.article_thread(path, cx);
                    let chat = id.and_then(|id| workspace.session(id, cx));
                    let point = Pointing::of(chat, &agents, cx).under_article(first);
                    return match (chat, workspace.article_offer(path, cx)) {
                        (None, Some((_, offers))) => point.offering(&agents, first, &offers),
                        _ => point,
                    };
                }
                let id = workspace
                    .showing_of(&pane.read(cx).entry, cx)
                    .and_then(|(_, showing)| match showing {
                        Showing::Session(id) => Some(id),
                        _ => None,
                    });
                Pointing::of(id.and_then(|id| workspace.session(id, cx)), &agents, cx)
            })
            .collect();
        // Read out before writing: the composers are updated through `cx`,
        // which the workspace is borrowed from.
        let tabs = workspace.settings.features.panel;
        let threads: Vec<Option<gpui::AnyView>> = articles
            .into_iter()
            .map(|article| article.map(|path| self.thread_view(path, cx).into()))
            .collect();
        for ((pane, point), thread) in self.leaves.clone().iter().zip(pointed).zip(threads) {
            let composer = pane.read(cx).composer.clone();
            composer.update(cx, |composer, cx| {
                composer.set_tools(!arranged, cx);
                composer.set_panel_tabs(tabs, cx);
                composer.set_thread(thread, cx);
                point.apply(composer, cx);
            });
        }
        self.sync_session_cards(cx);
    }

    /// The thread the composer under the article at `path` opens.
    fn thread_view(
        &mut self,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<crate::view::article::thread::ThreadView> {
        if let Some(view) = self.threads.get(&path) {
            return view.clone();
        }
        let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
        let view = cx.new(|cx| {
            crate::view::article::thread::ThreadView::new(workspace, root, path.clone(), cx)
        });
        self.threads.insert(path, view.clone());
        view
    }

    /// What a press in the composer under the article at `path` asks for. The
    /// first send opens the article's session.
    pub(crate) fn thread_event(
        &mut self,
        path: &Path,
        event: &composer::ComposerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = match event {
            composer::ComposerEvent::Submit(..) | composer::ComposerEvent::Reconnect => self
                .workspace
                .update(cx, |workspace, cx| workspace.article_session(path, cx)),
            _ => self.workspace.read(cx).article_thread(path, cx),
        };
        if let composer::ComposerEvent::Switch(id, value) = event
            && session.is_none()
        {
            let change = match id {
                composer::SwitchId::Agent => value.parse().ok().map(ArticleChange::Agent),
                composer::SwitchId::Mode => Some(ArticleChange::Mode(value.to_string())),
                composer::SwitchId::Config(config) => Some(ArticleChange::Config {
                    id: config.to_string(),
                    value: value.to_string(),
                }),
            };
            let cwd = self.workspace.read(cx).article_cwd(path);
            if let (Some(change), Some(cwd)) = (change, cwd) {
                self.workspace.update(cx, |workspace, cx| {
                    workspace.set_article_default(&cwd, change, cx)
                });
            }
            return;
        }
        self.composer_event(session, event, window, cx);
    }

    /// Open another session under the article at `path`, once the one it
    /// shows has been said something in.
    pub(crate) fn new_thread(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            let said = workspace
                .article_thread(path, cx)
                .and_then(|id| workspace.session(id, cx))
                .is_some_and(|chat| !chat.unsaid());
            if said {
                workspace.new_article_session(path, cx);
            }
        });
    }

    /// The card the library or the launch view stands in, under the window's
    /// top row — see [`Cydonia::window_band`].
    fn lone_card(
        &mut self,
        library: Option<gpui::Entity<crate::view::library::Library>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let floating = self.workspace.read(cx).settings.appearance.floating;
        let body = match library.clone() {
            Some(library) => library
                .cached(
                    gpui::StyleRefinement::default()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col(),
                )
                .into_any_element(),
            None => self.launch(window, cx),
        };
        // The library's drawer, under the band like the body. A pane draws its
        // own.
        let drawer = match library {
            Some(_) => self.drawer_layer(None, window, cx),
            None => Vec::new(),
        };
        bezel::ui::window::content(floating, cx)
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .capture_any_mouse_down(cx.listener(|this, _, _, _| this.pressed_pane = None))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(body),
            )
            .when(!drawer.is_empty(), |card| {
                card.child(div().absolute().inset_0().children(drawer))
            })
    }

    pub(crate) fn detail(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        // The library stands in for every pane, framed as the launch view is:
        // one card under the window's band, the drawer a card opens in, and no
        // composer.
        let library = self.library.clone();
        let panes = match library {
            Some(_) => None,
            None => self.panes(window, cx),
        };
        // The window's top row, over every column the panel's included: the
        // strip over a tab, the band over the library or the launch view.
        // Floating, it is on the chrome over the cards. Edge to edge either
        // way: its ends are the window's, as the lights are. None where the
        // panes' title rows are it — see [`Cydonia::sync_edges`].
        let on_top = self.panes_on_top(cx);
        let height = chrome::top_height(self.workspace.read(cx).settings.appearance.global_tabs());
        let top = (!on_top).then(|| {
            div()
                .flex_none()
                .relative()
                .h(px(height))
                .when(!floating, |top| {
                    top.bg(bezel::ui::window::content_fill(false, &theme))
                })
                .child(match panes.is_some() {
                    true => self
                        .strip
                        .clone()
                        .cached(gpui::StyleRefinement::default().w_full().h(px(height)))
                        .into_any_element(),
                    false => self.window_band(window, cx),
                })
        });
        let main = match (library, panes) {
            // The entry or the space in front.
            (_, Some(panes)) => match floating {
                true => div(),
                false => bezel::ui::window::content(false, cx),
            }
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .flex()
            .flex_col()
            .child(panes),
            (library, None) => self.lone_card(library, window, cx),
        };
        // The one panel the window has, under whatever is showing: a space's
        // panes included, which is what the right panel cannot do.
        #[cfg(feature = "desktop")]
        let terminal = self
            .terminal
            .as_ref()
            .filter(|(visible, _)| *visible)
            .map(|(_, terminal)| gpui::AnyView::from(terminal.clone()));
        #[cfg(not(feature = "desktop"))]
        let terminal: Option<gpui::AnyView> = None;
        let changes = self.changes.clone();
        let available = f32::from(window.viewport_size().width)
            - if self.sidebar_open {
                self.sidebar_width
            } else {
                0.
            };
        let available = available.max(0.);
        // Beside the chat, or over it in a window too narrow to hold both.
        let beside = panel_beside(available);
        let width = panel_width(self.changes_share, available);
        let height = panel_height(
            self.terminal_height,
            f32::from(window.viewport_size().height),
        );
        let panels = div()
            .id("session-panels")
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<TerminalResize>, _, cx| {
                    let gap = root::card_gap(this.workspace.read(cx).settings.appearance.floating);
                    this.terminal_height = Some(panel_height(
                        Some(f32::from(event.bounds.bottom() - event.event.position.y) - gap),
                        f32::from(event.bounds.size.height),
                    ));
                    this.save_panel_layout_settled(cx);
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
                            let available = f32::from(event.bounds.size.width);
                            let gap = root::card_gap(
                                this.workspace.read(cx).settings.appearance.floating,
                            );
                            let dragged =
                                f32::from(event.bounds.right() - event.event.position.x) - gap;
                            if available > 0. {
                                this.changes_share = Some(
                                    panel_width(Some(dragged / available), available) / available,
                                );
                            }
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
                            .flex()
                            .flex_col()
                            .child(panel)
                    }))
                    // Over the chat rather than beside it. The chat stays in
                    // the tree behind it, so what it was scrolled to and what
                    // was typed into it are still there when the window is
                    // widened again — and the panel is laid on the content's
                    // own fill, so nothing reads through.
                    .children(changes.clone().filter(|_| !beside).map(|panel| {
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_col()
                            .bg(bezel::ui::window::content_fill(floating, &theme))
                            .child(panel)
                    }))
                    // No split to drag when there is nothing beside anything.
                    .when(changes.is_some() && beside, |row| {
                        row.child(
                            crate::view::component::divider::seam(
                                &theme,
                                Axis::Horizontal,
                                floating,
                            )
                            .id("changes-split")
                            .absolute()
                            .top_0()
                            .right(px(width + root::card_gap(floating)
                                - crate::view::component::divider::HIT / 2.))
                            .on_drag(ChangesResize, |_, _, _, cx| cx.new(|_| Empty)),
                        )
                    }),
            )
            .children(terminal.clone().map(|terminal| {
                bezel::ui::window::content(floating, cx)
                    .h(px(height))
                    .flex_none()
                    .child(terminal)
            }))
            .when(terminal.is_some(), |column| {
                column.child(
                    crate::view::component::divider::seam(&theme, Axis::Vertical, floating)
                        .id("terminal-split")
                        .absolute()
                        .left_0()
                        .bottom(px(height + root::card_gap(floating)
                            - crate::view::component::divider::HIT / 2.))
                        .on_drag(TerminalResize, |_, _, _, cx| cx.new(|_| Empty)),
                )
            });
        // Floating, the cards leave the column half the inset to pad itself by.
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .children(top)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(floating, |column| {
                        column
                            .px(px(Theme::CONTENT_INSET / 2.))
                            .pb(px(Theme::CONTENT_INSET / 2.))
                    })
                    .child(panels),
            )
            .into_any_element()
    }
}

/// A session pane's drop target while a file is dragged over it: a wash over
/// the whole pane, composer included. Drawn last; it takes no drop itself.
pub(crate) fn drop_wash(theme: &Theme) -> Div {
    let wash = theme.drop_target;
    div()
        .absolute()
        .inset_0()
        .drag_over::<gpui::ExternalPaths>(move |style, _, _, _| style.bg(wash))
}

/// The agents a composer's mark is drawn from, in settings order.
pub(crate) fn composer_agents(workspace: &Workspace) -> Vec<composer::Agent> {
    workspace
        .settings
        .agents
        .iter()
        .map(|entry| composer::Agent {
            name: entry.name.clone().into(),
            icon: workspace.agent_icon(&entry.name),
        })
        .collect()
}

/// What a composer shows of the session it is pointed at, read out of the
/// workspace before any composer is written.
pub(crate) struct Pointing {
    session: Option<u64>,
    draft: String,
    placeholder: String,
    commands: Vec<Command>,
    streaming: bool,
    last_asked: Option<String>,
    activity: Option<composer::Activity>,
    agents: Vec<composer::Agent>,
    current: Option<usize>,
    switches: Vec<composer::Switch>,
    usage: Option<Usage>,
}

impl Pointing {
    pub(crate) fn of(chat: Option<&ChatSession>, agents: &[composer::Agent], cx: &App) -> Self {
        // Usage belongs to the agent process rather than to the transcript, so
        // a session read back off disk shows none until it reconnects.
        let live = chat.filter(|chat| chat.live());
        Self {
            session: chat.map(|chat| chat.id),
            draft: chat.map(|chat| chat.draft.clone()).unwrap_or_default(),
            placeholder: chat.map_or_else(
                || "message the agent…".to_owned(),
                |chat| format!("message {}…", chat.entry.name),
            ),
            commands: chat.map(|chat| chat.commands.clone()).unwrap_or_default(),
            streaming: chat.is_some_and(|chat| chat.streaming),
            last_asked: chat.and_then(|chat| {
                chat.items.iter().rev().find_map(|item| match item {
                    ChatItem::User(text) => Some(text.clone()),
                    _ => None,
                })
            }),
            activity: chat.and_then(composer::Activity::of),
            agents: agents.to_vec(),
            current: chat.and_then(|chat| {
                agents
                    .iter()
                    .position(|agent| agent.name == chat.entry.name)
            }),
            switches: chat.map_or_else(Vec::new, |chat| switches_of(&chat.offered(cx))),
            usage: live.and_then(|chat| chat.usage),
        }
    }

    /// Pointed at an article's session. With none opened yet, the mark is
    /// `first`, the agent the first send opens one on.
    pub(crate) fn under_article(mut self, first: Option<usize>) -> Self {
        self.placeholder = "Ask about this article…".to_owned();
        if self.session.is_none() {
            self.current = first;
        }
        self
    }

    /// Pointed at an article with no session yet: its default agent's offers,
    /// with the agent itself as a switch in front of them.
    pub(crate) fn offering(
        mut self,
        agents: &[composer::Agent],
        current: Option<usize>,
        offers: &Offers,
    ) -> Self {
        let agent = composer::Switch {
            id: composer::SwitchId::Agent,
            name: "Agent".into(),
            current: current.map(|ix| ix.to_string().into()),
            options: agents
                .iter()
                .enumerate()
                .map(|(ix, agent)| composer::SwitchOption {
                    id: ix.to_string().into(),
                    name: agent.name.clone(),
                })
                .collect(),
        };
        self.switches = std::iter::once(agent).chain(switches_of(offers)).collect();
        self
    }

    pub(crate) fn apply(
        self,
        composer: &mut composer::Composer,
        cx: &mut Context<composer::Composer>,
    ) {
        composer.set_session(self.session, &self.draft, cx);
        composer.set_placeholder(&self.placeholder, cx);
        composer.set_commands(&self.commands, cx);
        composer.set_streaming(self.streaming, cx);
        composer.set_last_asked(self.last_asked);
        composer.set_activity(self.activity, cx);
        composer.set_agents(&self.agents, self.current, cx);
        composer.set_switches(&self.switches, cx);
        composer.set_usage(self.usage, cx);
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
                                // Prepaint is mid-draw, where `refresh` is
                                // dropped. The transcript reading this height
                                // is drawn by the view this canvas is in.
                                window.request_animation_frame();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
        )
}

/// The invitation's rows as one block: stretched to the widest so every row's
/// hover rect is the same width and every glyph lands on the same edge, and
/// held off the line above it — `empty_state` centres its children, which
/// would otherwise centre each row on its own width. The rows sit a menu's
/// 2px apart rather than further: they are padded press targets, and spacing
/// them like paragraphs would read as three separate invitations.
fn make_list(rows: impl IntoIterator<Item = AnyElement>) -> impl IntoElement {
    div()
        .mt(px(22.))
        .flex()
        .flex_col()
        .items_stretch()
        .gap(px(2.))
        .children(rows)
}

impl Cydonia {
    /// The front door, and what stands where a pane would be if one were
    /// showing: a project to open, or the first entry to make in the one that
    /// already is.
    pub(crate) fn launch(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.workspace.read(cx).active_project().is_some() {
            true => self.nothing_open(window, cx),
            false => self.no_project(cx),
        }
    }

    /// The agents a new session can run on, from the front door's session
    /// row.
    fn launch_rows(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Vec<(Item, menu::Act)> {
        new::offered(self.workspace.read(cx))
            .agents
            .into_iter()
            .enumerate()
            .map(|(at, (name, icon))| {
                menu::row::<Self>(
                    Item::action(name).with_icon(icon),
                    move |this, window, cx| this.pick_agent(at, window, cx),
                )
            })
            .collect()
    }

    /// What to do when there is nothing to show: make the first entry in the
    /// project that is open, one row per kind that is switched on.
    fn nothing_open(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(ix) = workspace.active else {
            return self.no_project(cx);
        };
        let name = workspace
            .projects
            .get(ix)
            .map(|project| project.path.shown())
            .unwrap_or_default();
        let offered = new::offered(workspace);
        let mut rows: Vec<AnyElement> = Vec::new();
        // One row, and a panel of agents under it once there is a choice to
        // make: a bare "New session" opens on whichever agent is first, and
        // nothing on this screen would say which.
        let session = New::Session(None);
        if offered.sessions && offered.agents.len() > 1 {
            let trigger = self.make_row(
                "session",
                session.label(),
                session.icon(),
                keys::shortcut(&NewSession, window),
                cx,
                move |this, at, window, cx| {
                    let place = at.map_or(Place::Below, Place::At);
                    this.toggle_menu(Menu::Launch, place, Self::launch_rows, window, cx)
                },
            );
            rows.push(
                self.menu_trigger(trigger, Menu::Launch, cx)
                    .relative()
                    .children(self.menu_here(&Menu::Launch, "launch-menu", window, cx))
                    .into_any_element(),
            );
        } else if offered.sessions {
            rows.push(
                self.make_row(
                    "session",
                    session.label(),
                    session.icon(),
                    keys::shortcut(&NewSession, window),
                    cx,
                    move |this, _, window, cx| this.new_session_action(&NewSession, window, cx),
                )
                .into_any_element(),
            );
        }
        for kind in offered.kinds {
            let (id, chord) = match kind {
                New::Board => ("board", keys::shortcut(&NewBoard, window)),
                New::Article => ("article", keys::shortcut(&NewArticle, window)),
                New::Table => ("table", keys::shortcut(&NewTable, window)),
                New::Session(_) => continue,
            };
            rows.push(
                self.make_row(
                    id,
                    kind.label(),
                    kind.icon(),
                    chord,
                    cx,
                    move |this, _, window, cx| match kind {
                        New::Board => this.ask_new_board(ix, window, cx),
                        New::Article => this.new_article(ix, window, cx),
                        New::Table => this.new_table(ix, window, cx),
                        New::Session(_) => {}
                    },
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

    /// One line of the invitation: a glyph, a label, the chord that does the
    /// same thing where there is one, and what it makes. A menu row's metrics
    /// and hover rect, because that is what it is — the empty state is the
    /// only place these four live together, and bare text at this size reads
    /// as a sentence rather than a thing to press.
    fn make_row(
        &self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        glyph: impl Into<Icon>,
        chord: Option<SharedString>,
        cx: &mut Context<Self>,
        make: impl Fn(&mut Self, Option<Point<Pixels>>, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let (id, label) = (id.into(), label.into());
        let fade = Fade::new(Painter::of(cx), format!("launch-{id}"));
        // An svg paints in its own `text_color` and inherits none, so the glyph
        // cannot ride the row's fade. It takes the row's group instead — the
        // group is named per row so hovering one does not light the rest.
        popover::menu_row(&theme, false, Some(fade))
            .id(id.clone())
            .group(id.clone())
            .child(
                icons::icon(glyph)
                    .size(px(15.))
                    .flex_none()
                    .text_color(theme.text_muted)
                    .group_hover(id, |el| el.text_color(theme.text)),
            )
            .child(div().flex_1().child(label))
            .children(chord.map(|chord| popover::kbd_hint(&theme, chord)))
            // The press carries where it landed, for a row whose menu stands
            // at the pointer. `None` off the keyboard, which has no point to
            // it.
            .on_click(
                cx.listener(move |this, click: &gpui::ClickEvent, window, cx| {
                    make(this, click.mouse_position(), window, cx)
                }),
            )
    }

    /// The session view `on`'s pane draws, where it shows a session.
    fn session_view_of(
        &self,
        on: Option<&Member>,
        cx: &App,
    ) -> Option<gpui::Entity<transcript::SessionView>> {
        self.leaf_of(on, cx)?.read(cx).session_shown(cx)
    }

    /// The view of the session `member` names in the pane showing it.
    pub(crate) fn session_scroll(
        &self,
        member: &Member,
        cx: &App,
    ) -> Option<gpui::Entity<transcript::SessionView>> {
        let id = self.workspace.read(cx).session_id_of(Some(member), cx)?;
        self.leaves
            .iter()
            .map(|pane| pane.read(cx))
            .filter(|pane| pane.entry == *member)
            .flat_map(|pane| pane.sessions.iter())
            .find(|view| view.read(cx).session == id)
            .cloned()
    }

    /// Every pane's view of session `id`.
    pub(crate) fn session_views(
        &self,
        id: u64,
        cx: &App,
    ) -> Vec<gpui::Entity<transcript::SessionView>> {
        self.leaves
            .iter()
            .flat_map(|pane| pane.read(cx).sessions.iter())
            .filter(|view| view.read(cx).session == id)
            .cloned()
            .collect()
    }

    /// Scroll the focused pane's view of session `id` to turn `turn`, from 0.
    pub(crate) fn reveal_session_turn(&mut self, id: u64, turn: usize, cx: &mut Context<Self>) {
        self.sync_items(cx);
        if let Some(view) = self
            .session_view_of(self.focused_entry(cx).as_ref(), cx)
            .filter(|view| view.read(cx).session == id)
        {
            view.read(cx).state.reveal_turn(turn);
        }
    }

    /// Put what the focused pane's transcript has selected on the clipboard.
    /// Answers whether there was anything.
    pub(crate) fn copy_transcript_selection(&mut self, cx: &mut Context<Self>) -> bool {
        self.leaf()
            .cloned()
            .is_some_and(|pane| pane.update(cx, |pane, cx| pane.copy_transcript_selection(cx)))
    }

    /// The queue under session `id`'s transcript in `on`'s pane — see
    /// [`Tab::queue`].
    pub(crate) fn queue(
        &mut self,
        id: u64,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.leaf_of(on, cx).cloned()?.update(cx, |pane, cx| {
            pane.queue(id, window, cx)
                .map(IntoElement::into_any_element)
        })
    }

    /// A pick from one of the composer's switches — the session's mode, or a
    /// config option like the model.
    fn switch(
        &mut self,
        session: u64,
        id: &composer::SwitchId,
        value: &SharedString,
        cx: &mut Context<Self>,
    ) {
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
            // Only offered before a session exists; see `thread_event`.
            composer::SwitchId::Agent => {}
        });
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/queued_images.rs"]
mod queued_image_tests;

#[cfg(test)]
#[path = "../../tests/unit/composer_footer.rs"]
mod composer_footer_tests;
