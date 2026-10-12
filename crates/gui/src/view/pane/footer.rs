//! What a pane shows of a session: the transcript with its empty states, the
//! band floating at its foot — the agent's plan, the permission it asks for,
//! and the composer — the queue of prompts waiting for a turn, and the bar
//! over a run picked out of the transcript.

use super::Tab;
use crate::view::kit::text::ShownPath as _;
use crate::view::window::reach::InWindow as _;
use crate::{
    model::workspace::Workspace,
    view::{
        component::{composer, ribbon, transcript},
        detail::{adrift, adrift_line, agent_missing, footer},
        window::section::Section,
    },
};
use artifact::session::chat::PlanStatus;
use bezel::{
    gpui::{
        self, AnyElement, App, AppContext as _, Context, Entity, Focusable as _, IntoElement,
        Pixels, SharedString, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Controls, Status},
    },
};
use cacp::schema::PermissionOptionKind;
use std::{cell::Cell, rc::Rc};

/// What a press on the band does.
pub(crate) type Press = Rc<dyn Fn(&mut Window, &mut App)>;

/// What floats at the foot of a session's transcript: the agent's plan, any
/// permission it is asking for, and a composer.
pub(crate) struct Band {
    workspace: Entity<Workspace>,
    session: Option<u64>,
}

impl Band {
    pub(crate) fn new(workspace: Entity<Workspace>, session: Option<u64>) -> Self {
        Self { workspace, session }
    }

    /// The band over `composer`, measured into `height`.
    pub(crate) fn render<V: 'static>(
        &self,
        composer: Entity<composer::Composer>,
        press: Option<Press>,
        height: Option<Rc<Cell<Pixels>>>,
        cx: &Context<V>,
    ) -> AnyElement {
        footer(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .when_some(press, |band, press| {
                    band.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        press(window, cx)
                    })
                })
                .children(self.plan(cx))
                .children(self.permission(cx))
                .child(composer),
            height,
        )
        .into_any_element()
    }

    /// The agent's plan, while it still has something left to do.
    fn plan(&self, cx: &App) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).session(self.session?, cx)?;
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
    /// a checkbox saying how long the answer holds. See
    /// [`crate::view::detail::alert`] for why two buttons carry four options,
    /// and for what an agent has to ask to get the stack of rows instead.
    fn permission<V: 'static>(&self, cx: &Context<V>) -> Option<impl IntoElement + use<V>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).session(self.session?, cx)?;
        let prompt = chat.permission.as_ref()?;
        let id = chat.id;
        let painter = Painter::of(cx);
        // One button, whichever space it lands in. `key` is the element's and
        // the hover wash's both — the wash store is one map for the whole app,
        // so the session is in it too.
        let answer = |key: &str, option_id: String, label: &str, style| {
            let fade = Fade::new(painter, format!("permission-{id}-{key}"));
            let workspace = self.workspace.clone();
            theme
                .button(label.to_owned(), style, Some(fade))
                .id(SharedString::from(key.to_owned()))
                .on_click(move |_, _, cx| {
                    let option_id = option_id.clone();
                    workspace.update(cx, |workspace, cx| {
                        workspace.with_session(id, cx, |chat| chat.respond_permission(option_id));
                    });
                })
        };
        let always = self.workspace.clone();
        let body = match crate::view::detail::alert(&prompt.options) {
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
                        .on_click(move |_, _, cx| {
                            always.update(cx, |workspace, cx| {
                                workspace
                                    .with_session(id, cx, |chat| chat.toggle_permission_always());
                            });
                        })
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
}

impl Tab {
    /// A press on the pane's band: the focus comes to the pane.
    fn press(&self) -> Option<Press> {
        let on = self.entry.clone()?;
        let root = self.root.clone();
        Some(Rc::new(move |window, cx| {
            root.update(cx, |root, cx| root.focus_pane(&on, window, cx))
                .ok();
        }))
    }

    /// The band under session `session`'s transcript, over the pane's own
    /// composer.
    pub(crate) fn session_footer(&self, session: Option<u64>, cx: &Context<Self>) -> AnyElement {
        let height = self
            .session_shown(cx)
            .map(|view| view.read(cx).state.footer_height.clone());
        Band::new(self.workspace.clone(), session).render(
            self.composer.clone(),
            self.press(),
            height,
            cx,
        )
    }

    /// The composer under the article the pane shows, pointed at the article's
    /// session. Nothing while article threads are off, or with nowhere to send.
    pub(crate) fn thread_footer(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let path = self.thread_article(cx)?;
        let workspace = self.workspace.read(cx);
        if !workspace.thread_reachable(&path, cx) {
            return None;
        }
        let session = workspace.article_thread(&path, cx);
        // The article's own measure, not the session's: the article's page and
        // outline keep clear of it.
        let height = self
            .article_shown(cx)
            .map(|view| view.read(cx).composer.clone());
        Some(Band::new(self.workspace.clone(), session).render(
            self.composer.clone(),
            self.press(),
            height,
            cx,
        ))
    }

    /// What stands where the composer would be over a transcript there is no
    /// longer anywhere to answer into. The strip is the click: it names where
    /// the agent is installed, and going there is all it has to offer.
    ///
    /// Nothing at all where the session has said nothing — [`Self::no_agent`]
    /// is the whole pane then, and this under it would say it twice.
    pub(crate) fn adrift_strip(
        &self,
        session: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let chat = workspace
            .session(session?, cx)
            .filter(|chat| !chat.unsaid())?;
        let message = adrift_line(&chat.entry.name, !workspace.settings.agents.is_empty());
        Some(
            theme
                .warning_strip(message)
                .id("adrift")
                .mt(px(0.))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.defer_window(window, cx, |root, window, cx| {
                        root.open_settings(Section::Agents, window, cx)
                    });
                }))
                .into_any_element(),
        )
    }

    /// Session `id`'s transcript, or what stands in for it where there is
    /// nothing to show yet or nowhere to send.
    pub(crate) fn conversation(&self, id: Option<u64>, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let Some(chat) = id.and_then(|id| workspace.session(id, cx)) else {
            // No session, and the pane showing regardless: one was asked for
            // with no agent to open it on — see [`Tab::asked_session`].
            return match self.asked_session {
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
                .map(|project| project.path.shown())
                .unwrap_or_default();
            // Except where the agent is not here to be started in it. A
            // transcript that has nothing to show *and* nowhere to send is the
            // whole pane, so the way out goes here rather than under it.
            let others = !workspace.settings.agents.is_empty();
            if !workspace.reachable(cx) {
                return self.no_agent(Some(&agent), others, cx);
            }
            return theme
                .empty_state(icons::files::Folder, cwd, format!("{agent} runs here"))
                .flex_1()
                .into_any_element();
        }
        let body = self
            .session_shown(cx)
            .map(|view| view.into_any_element())
            .unwrap_or_else(|| div().flex_1().into_any_element());
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(body)
            .children(self.search_pill(cx))
            .into_any_element()
    }

    /// No agent to say anything to: what is missing, and the way to put it
    /// there. The whole pane, because there is nothing else in it.
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
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.defer_window(window, cx, |root, window, cx| {
                            root.open_settings(Section::Agents, window, cx)
                        });
                    })),
            )
            .into_any_element()
    }

    /// The bar over a run picked out of the transcript: what to do with it,
    /// where it was picked.
    ///
    /// The article's ribbon in a second place — same perch, same layer, and
    /// the same rule that a bar belongs to a run rather than to a pointer, so
    /// it arrives when the drag ends and goes when the run does.
    pub(crate) fn selection_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (view, head) = self.session_shown(cx)?.read(cx).state.selection_perch()?;
        let at = ribbon::perch(view, head)?;
        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .child(ribbon::action(
                "Copy",
                icons::text::Copy,
                &theme,
                cx,
                |this: &mut Self, _, cx| {
                    this.copy_transcript_selection(cx);
                    this.clear_transcript_selection(cx);
                },
            ))
            .child(ribbon::action(
                "Quote",
                icons::text::TextQuote,
                &theme,
                cx,
                |this: &mut Self, window, cx| this.quote_selection(window, cx),
            ))
            .child(ribbon::action(
                "Link",
                icons::text::Link,
                &theme,
                cx,
                |this: &mut Self, _, cx| {
                    let turn = this.session_shown(cx).and_then(|view| {
                        let view = view.read(cx);
                        Some((view.session, view.selected_turn(cx)? + 1))
                    });
                    let link = turn.and_then(|(id, turn)| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.turn_link(id, turn, turn, cx))
                    });
                    if let Some(link) = link {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(link));
                    }
                    this.clear_transcript_selection(cx);
                },
            ));
        Some(ribbon::floated(
            "transcript-selection",
            at,
            card.into_any_element(),
        ))
    }

    /// Put what the transcript has selected on the clipboard. Answers whether
    /// there was anything.
    pub(crate) fn copy_transcript_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self
            .session_shown(cx)
            .and_then(|view| view.read(cx).copied(cx))
        else {
            return false;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        true
    }

    /// Drop the run the transcript is holding, and the bar over it.
    pub(crate) fn clear_transcript_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.session_shown(cx) {
            view.update(cx, |view, cx| {
                view.state.clear_selection();
                cx.notify();
            });
        }
    }

    /// Answer the run: the composer takes it, and the caret goes where the
    /// reply is written. The run is dropped with it — the bar has done what it
    /// was for, and a highlight with nothing over it reads as half a state.
    fn quote_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .session_shown(cx)
            .and_then(|view| view.read(cx).copied(cx));
        let Some(text) = text else {
            return;
        };
        self.composer
            .update(cx, |composer, cx| composer.quote(text, cx));
        self.clear_transcript_selection(cx);
        window.focus(&self.composer.focus_handle(cx), cx);
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
    pub(crate) fn queue(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        let chat = self.workspace.read(cx).session(id, cx)?;
        let cwd = chat.cwd.clone();
        let queue = chat.queue.clone();
        self.queued_galleries
            .retain(|(session, ix, text), _| *session == id && queue.get(*ix) == Some(text));
        if queue.is_empty() {
            return None;
        }
        let mut rows = Vec::with_capacity(queue.len());
        for (ix, text) in queue.iter().enumerate() {
            let edit_text = text.clone();
            let cancel_text = text.clone();
            let (doc, images) = transcript::gallery::document(text);
            let gallery = (!images.is_empty()).then(|| {
                self.queued_galleries
                    .entry((id, ix, text.clone()))
                    .or_insert_with(|| {
                        cx.new(|cx| transcript::gallery::Gallery::new(images, &cwd, cx))
                    })
                    .clone()
            });
            rows.push(
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
                                bubble.child(markdown::render(&doc, Default::default(), window, cx))
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
                                        let Some(text) = this.take_queued(id, ix, &edit_text, cx)
                                        else {
                                            return;
                                        };
                                        // Back into the composer it was typed
                                        // in, which is the one under this pane.
                                        this.composer.update(cx, |composer, cx| {
                                            composer.restore(text, cx);
                                            window.focus(&composer.focus_handle(cx), cx);
                                        });
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
                    ),
            );
        }
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .children(rows),
        )
    }
}
