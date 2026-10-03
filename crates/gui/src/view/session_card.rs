//! A session painted into an article: a ` ```session ` fence holding the
//! session's link, drawn as its transcript over a composer of its own.
//!
//! An empty fence is the picker that writes the link in. The fence holds the
//! session's record; deleting the block leaves the session where it was.

use crate::view::{
    component::{
        composer::{Composer, ComposerEvent},
        menu::{self, Menu},
        transcript,
    },
    root::Cydonia,
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, MouseButton, SharedString, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        menu::Item,
        popover,
        widgets::{ButtonStyle, Buttons},
    },
};

/// The fence's info string.
pub(crate) const LANGUAGE: &str = "session";
const SCHEME: &str = "cydonia://session/";
/// The transcript's box. It scrolls inside; the article does not grow with
/// the conversation.
const TRANSCRIPT_HEIGHT: f32 = 320.;
/// What the transcript lays its column out against.
const CARD_WIDTH: f32 = 720.;

/// The link a fence holds for the session filed under `record`.
pub(crate) fn link(record: &str) -> String {
    format!("{SCHEME}{record}")
}

fn record_of(code: &str) -> Option<&str> {
    code.trim()
        .strip_prefix(SCHEME)
        .filter(|record| !record.is_empty())
}

/// The block renderer cydonia installs at boot.
pub(crate) fn render(
    fence: &markdown::Fence<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    if fence.language != LANGUAGE {
        return None;
    }
    let root = window.root::<Cydonia>().flatten()?;
    Some(root.update(cx, |root, cx| root.session_card(fence, window, cx)))
}

impl Cydonia {
    fn session_card(
        &mut self,
        fence: &markdown::Fence<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let body = match record_of(fence.code) {
            None => self.session_picker(fence, window, cx),
            Some(record) => {
                let found = self
                    .workspace
                    .read(cx)
                    .session_by_record(record)
                    .map(|chat| chat.id);
                match found {
                    Some(id) => self.session_live(record, id, window, cx),
                    None => div()
                        .p(px(12.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_faint)
                        .child(format!("No session filed as {record}"))
                        .into_any_element(),
                }
            }
        };
        div()
            .id(SharedString::from(format!(
                "session-card-{}",
                fence.code.trim()
            )))
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .overflow_hidden()
            // The card's presses are the card's: the editor would otherwise
            // put its caret in the fence and show the link instead.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(body)
            .into_any_element()
    }

    /// The empty fence: start a session here, or point at one already filed.
    fn session_picker(
        &mut self,
        fence: &markdown::Fence<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(rewrite) = fence.rewrite.clone() else {
            return div()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("No session linked")
                .into_any_element();
        };
        let made = rewrite.clone();
        let new = theme
            .ghost("session-card-new")
            .px(px(12.))
            .py(px(8.))
            .gap(px(6.))
            .child(
                icons::icon(icons::math::Plus)
                    .size(px(12.))
                    .text_color(theme.text_faint),
            )
            .child(
                div()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child("New session"),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                let record = this.workspace.update(cx, |workspace, cx| {
                    let entry = workspace.preferred_agent()?;
                    let id = workspace.new_session_behind(entry, cx)?;
                    workspace.mint_record(id)
                });
                if let Some(record) = record {
                    made(link(&record), window, cx);
                }
            }));
        let sessions: Vec<(String, String)> = self
            .workspace
            .read(cx)
            .active_project()
            .map(|project| {
                project
                    .sessions
                    .iter()
                    .filter_map(|chat| Some((chat.record.clone()?, chat.title.clone())))
                    .collect()
            })
            .unwrap_or_default();
        let rows = sessions
            .into_iter()
            .map(|(record, title)| {
                let rewrite = rewrite.clone();
                let label = match title.trim() {
                    "" => "Untitled session".to_owned(),
                    title => title.to_owned(),
                };
                menu::row(Item::action(label), move |_, window, cx| {
                    rewrite(link(&record), window, cx)
                })
            })
            .collect::<Vec<_>>();
        let key = SharedString::from("session-card-link");
        let linked = (!rows.is_empty() && self.menu == Some(Menu::SessionLink)).then(|| {
            popover::anchored_menu_below(
                key.clone(),
                self.menu_card(key.clone(), rows, window, cx),
                None,
            )
        });
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(new)
            .child(
                self.menu_button(key.clone(), None, icons::text::Link, Menu::SessionLink, cx)
                    .mr(px(6.))
                    .children(linked),
            )
            .into_any_element()
    }

    /// A linked session: its name and state, its transcript, and a composer
    /// that sends to it.
    fn session_live(
        &mut self,
        record: &str,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let composer = self.session_composer(record, id, window, cx);
        let (title, agent, streaming) = self
            .workspace
            .read(cx)
            .session(id)
            .map(|chat| (chat.title.clone(), chat.entry.name.clone(), chat.streaming))
            .unwrap_or_default();
        let header = div()
            .h(px(36.))
            .px(px(12.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text)
                    .child(match title.trim() {
                        "" => "Untitled session".to_owned(),
                        title => title.to_owned(),
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_faint)
                    .child(match streaming {
                        true => format!("{agent} · working"),
                        false => agent,
                    }),
            )
            .child(
                theme
                    .icon_button(icons::layout::Maximize, ButtonStyle::Ghost, None)
                    .id(SharedString::from(format!("session-card-open-{record}")))
                    .flex_none()
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_session(id, window, cx)),
                    ),
            );
        let transcript = self.workspace.update(cx, |workspace, cx| {
            workspace
                .session(id)
                .map(|chat| transcript::render(chat, None, CARD_WIDTH, |_, _| None, window, cx))
        });
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .h(px(TRANSCRIPT_HEIGHT))
                    .flex()
                    .flex_col()
                    .children(transcript),
            )
            .child(div().p(px(8.)).child(composer))
            .into_any_element()
    }

    /// The composer the card for `record` types into, made the first time the
    /// card is drawn and synced by [`Cydonia::sync_composer`] after.
    fn session_composer(
        &mut self,
        record: &str,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<Composer> {
        if let Some(composer) = self.session_cards.get(record) {
            return composer.clone();
        }
        let composer = cx.new(Composer::new);
        let draft = self
            .workspace
            .read(cx)
            .session(id)
            .map(|chat| chat.draft.clone())
            .unwrap_or_default();
        composer.update(cx, |composer, cx| {
            composer.set_tools(false, cx);
            composer.set_session(Some(id), &draft, cx);
        });
        let filed = record.to_owned();
        cx.subscribe_in(
            &composer,
            window,
            move |this, _, event: &ComposerEvent, _, cx| {
                let Some(id) = this
                    .workspace
                    .read(cx)
                    .session_by_record(&filed)
                    .map(|chat| chat.id)
                else {
                    return;
                };
                match event {
                    ComposerEvent::Submit(text, attachments) => {
                        this.workspace
                            .update(cx, |workspace, cx| match attachments.is_empty() {
                                true => workspace.send(id, text.clone(), cx),
                                false => workspace.send_attached(id, text.clone(), attachments, cx),
                            });
                        if let Some(chat) = this.workspace.read(cx).session(id) {
                            chat.transcript.follow_tail();
                        }
                    }
                    ComposerEvent::Draft(id, draft) => {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.set_draft(*id, draft.clone(), cx)
                        });
                    }
                    ComposerEvent::Cancel => {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.with_session(id, cx, |chat| chat.cancel())
                        });
                    }
                    // TODO: reconnect and switches act on the active session;
                    // a card's composer does not offer them yet.
                    ComposerEvent::Reconnect
                    | ComposerEvent::Switch(..)
                    | ComposerEvent::Terminal
                    | ComposerEvent::Changes
                    | ComposerEvent::Files => {}
                }
            },
        )
        .detach();
        self.session_cards
            .insert(record.to_owned(), composer.clone());
        composer
    }

    /// Keep each card's composer on its session — the cards' half of
    /// [`Cydonia::sync_composer`].
    pub(crate) fn sync_session_cards(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let pointed: Vec<_> = self
            .session_cards
            .iter()
            .map(|(record, composer)| {
                let chat = workspace.session_by_record(record);
                (
                    composer.clone(),
                    chat.map(|chat| chat.id),
                    chat.map(|chat| chat.draft.clone()).unwrap_or_default(),
                    chat.map(|chat| chat.commands.clone()).unwrap_or_default(),
                    chat.is_some_and(|chat| chat.streaming),
                )
            })
            .collect();
        for (composer, id, draft, commands, streaming) in pointed {
            composer.update(cx, |composer, cx| {
                composer.set_session(id, &draft, cx);
                composer.set_commands(&commands, cx);
                composer.set_streaming(streaming, cx);
            });
        }
    }
}
