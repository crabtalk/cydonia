//! A session painted into an article: a ` ```session ` fence holding a
//! reference to it, `cydonia://<project>#<number>` — see
//! [`artifact::reference`]. A whole session is drawn live, with a composer
//! that sends to it; a run of its turns, `#43:5-7`, is drawn as those turns
//! alone.
//!
//! The fence names the session; deleting the block leaves the session where
//! it was.

use std::rc::Rc;

use crate::{
    model::{settings, workspace::Showing},
    view::{
        component::{
            composer::{Composer, ComposerEvent},
            transcript,
        },
        root::Cydonia,
        search::Linked,
        sidebar::Row,
    },
};
use artifact::reference::{self, Target, Turns};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, MouseButton, SharedString, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons},
    },
};
use editor::{SlashAction, SlashAt, SlashItem};
use markdown::{BlockKind, Text};

/// The fence's info string.
pub(crate) const LANGUAGE: &str = "session";
pub(crate) const SCHEME: &str = "cydonia://";
/// The transcript's box. It scrolls inside; the article does not grow with
/// the conversation.
const TRANSCRIPT_HEIGHT: f32 = 320.;
/// What the transcript lays its column out against.
const CARD_WIDTH: f32 = 720.;
/// How many of a session's last turns the link picker previews.
pub(crate) const PREVIEW_TURNS: usize = 3;

/// The link a reference is written as.
pub(crate) fn link(reference: &str) -> String {
    format!("{SCHEME}{reference}")
}

/// The fence a session reference is written into.
fn fence(reference: &str) -> BlockKind {
    BlockKind::Code {
        language: Some(LANGUAGE.to_owned()),
        code: Text::plain(link(reference)),
    }
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
    Some(root.update(cx, |root, cx| root.session_card(fence.code, window, cx)))
}

/// The slash rows: a new session on each agent, and one already running.
pub(crate) fn slash_items(agents: &[settings::Agent]) -> Vec<SlashItem> {
    let new = agents
        .iter()
        .map(|agent| {
            let name = agent.name.clone();
            let run = move |at: SlashAt, window: &mut Window, cx: &mut App| {
                let Some(root) = window.root::<Cydonia>().flatten() else {
                    return;
                };
                let made = root.update(cx, |root, cx| {
                    root.workspace.update(cx, |workspace, cx| {
                        let entry = workspace
                            .settings
                            .agents
                            .iter()
                            .find(|entry| entry.name == name)
                            .cloned()?;
                        let id = workspace.new_session_behind(entry, cx)?;
                        workspace.mint_record(id)?;
                        workspace.reference_of_session(id)
                    })
                });
                if let Some(reference) = made {
                    at.editor
                        .update(cx, |editor, cx| {
                            editor.place_block(at.block, fence(&reference), cx)
                        })
                        .ok();
                }
            };
            (
                SharedString::from(agent.name.clone()),
                SlashAction::Run(Rc::new(run)),
            )
        })
        .collect::<Vec<_>>();
    let existing = |at: SlashAt, window: &mut Window, cx: &mut App| {
        let Some(root) = window.root::<Cydonia>().flatten() else {
            return;
        };
        root.update(cx, |root, cx| {
            root.link_search(
                Rc::new(move |_, linked: Linked, _, cx| {
                    at.editor
                        .update(cx, |editor, cx| {
                            editor.place_block(at.block, fence(&linked.reference), cx)
                        })
                        .ok();
                }),
                window,
                cx,
            )
        });
    };
    let mut rows: Vec<(SharedString, SlashAction)> = new;
    rows.push(("Existing…".into(), SlashAction::Run(Rc::new(existing))));
    vec![SlashItem::Group {
        label: "Session".into(),
        rows,
    }]
}

/// The open project a reference names by its directory's name, and the
/// active one for a reference naming none.
fn named_project<'a>(
    workspace: &'a crate::model::workspace::Workspace,
    name: Option<&str>,
) -> Option<&'a crate::model::project::Project> {
    match name {
        Some(name) => workspace
            .projects
            .iter()
            .find(|project| project.path.file_name().is_some_and(|last| last == name)),
        None => workspace.active_project(),
    }
}

impl Cydonia {
    /// The session a fence's reference names, and the turns it asks for. A
    /// reference naming no project names the active one.
    fn named_session(&self, code: &str, cx: &App) -> Result<(u64, Option<Turns>), String> {
        let written = code.trim();
        let text = written.strip_prefix(SCHEME).unwrap_or(written);
        let Some(reference) = reference::parse(text) else {
            return Err(format!("{written} is not a session reference"));
        };
        let Target::Entry { number, turns } = reference.target else {
            return Err(format!("{text} is a card, not a session"));
        };
        let workspace = self.workspace.read(cx);
        let project = named_project(workspace, reference.project);
        let Some(project) = project else {
            return Err(format!("No open project for {text}"));
        };
        project
            .sessions
            .iter()
            .find(|chat| chat.number == Some(number))
            .map(|chat| (chat.id, turns))
            .ok_or_else(|| format!("No session {text}"))
    }

    /// Open the entry a reference names, `project#12` — the target of a
    /// `cydonia://` link.
    pub(crate) fn open_reference(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reference) = reference::parse(text) else {
            return;
        };
        let Target::Entry { number, .. } = reference.target else {
            return;
        };
        let row = {
            let workspace = self.workspace.read(cx);
            let project = named_project(workspace, reference.project);
            project.and_then(|project| {
                let showing = project
                    .sessions
                    .iter()
                    .find(|chat| chat.number == Some(number))
                    .map(|chat| Showing::Session(chat.id))
                    .or_else(|| {
                        project
                            .articles
                            .iter()
                            .find(|article| article.number == Some(number))
                            .map(|article| Showing::Article(article.id.clone()))
                    })
                    .or_else(|| {
                        project
                            .boards
                            .iter()
                            .find(|board| board.number == Some(number))
                            .map(|board| Showing::Board(board.id.clone()))
                    })?;
                Some(Row::Entry {
                    project: project.path.clone(),
                    showing,
                })
            })
        };
        if let Some(row) = row {
            self.open_row(&row, window, cx);
        }
    }

    fn session_card(
        &mut self,
        code: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let body = match self.named_session(code, cx) {
            Ok((id, None)) => self.session_live(id, window, cx),
            Ok((id, Some(turns))) => self.session_excerpt(id, turns, window, cx),
            Err(why) => div()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(why)
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("session-card-{}", code.trim())))
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .overflow_hidden()
            // The card's presses are the card's: the editor would otherwise
            // put its caret in the fence and show the reference instead.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(body)
            .into_any_element()
    }

    /// A session's name, its agent and whether it is working, and the button
    /// that opens it in a pane.
    fn session_header(&self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (title, agent, streaming) = self
            .workspace
            .read(cx)
            .session(id)
            .map(|chat| (chat.title.clone(), chat.entry.name.clone(), chat.streaming))
            .unwrap_or_default();
        div()
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
                    .id(SharedString::from(format!("session-card-open-{id}")))
                    .flex_none()
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_session(id, window, cx)),
                    ),
            )
            .into_any_element()
    }

    /// A whole session: its transcript, and a composer that sends to it.
    fn session_live(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let composer = self.session_composer(id, window, cx);
        let header = self.session_header(id, cx);
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
                    .id(SharedString::from(format!("session-card-transcript-{id}")))
                    .h(px(TRANSCRIPT_HEIGHT))
                    // The transcript's wheel stops here, at its ends too, so
                    // the article under the card does not scroll with it.
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .flex()
                    .flex_col()
                    .children(transcript),
            )
            .child(div().p(px(8.)).child(composer))
            .into_any_element()
    }

    /// A run of a session's turns, read only.
    fn session_excerpt(
        &mut self,
        id: u64,
        turns: Turns,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.session_header(id, cx);
        let body = self.workspace.update(cx, |workspace, cx| {
            workspace.session(id).map(|chat| {
                let from = turns.from.saturating_sub(1) as usize;
                transcript::excerpt(chat, from, turns.to as usize, window, cx)
            })
        });
        div()
            .flex()
            .flex_col()
            .child(header)
            .children(body)
            .into_any_element()
    }

    /// The composer the card for session `id` types into, made the first time
    /// the card is drawn and synced by [`Cydonia::sync_composer`] after.
    fn session_composer(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<Composer> {
        if let Some(composer) = self.session_cards.get(&id) {
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
        cx.subscribe_in(
            &composer,
            window,
            move |this, _, event: &ComposerEvent, _, cx| match event {
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
                // TODO: reconnect and switches act on the active session; a
                // card's composer does not offer them yet.
                ComposerEvent::Reconnect
                | ComposerEvent::Switch(..)
                | ComposerEvent::Terminal
                | ComposerEvent::Changes
                | ComposerEvent::Files => {}
            },
        )
        .detach();
        self.session_cards.insert(id, composer.clone());
        composer
    }

    /// Keep each card's composer on its session, and the slash menu's agents
    /// on the ones settings hold — the cards' half of
    /// [`Cydonia::sync_composer`].
    pub(crate) fn sync_session_cards(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        let items = slash_items(&workspace.settings.agents);
        let linkables = crate::view::mention::read(workspace);
        let pointed: Vec<_> = self
            .session_cards
            .iter()
            .map(|(id, composer)| {
                let chat = workspace.session(*id);
                (
                    composer.clone(),
                    chat.map(|chat| chat.draft.clone()).unwrap_or_default(),
                    chat.map(|chat| chat.commands.clone()).unwrap_or_default(),
                    chat.is_some_and(|chat| chat.streaming),
                )
            })
            .collect();
        editor::AppExt::set_slash_items(&mut **cx, items);
        cx.set_global(linkables);
        for (composer, draft, commands, streaming) in pointed {
            composer.update(cx, |composer, cx| {
                composer.set_session(composer.session(), &draft, cx);
                composer.set_commands(&commands, cx);
                composer.set_streaming(streaming, cx);
            });
        }
    }
}
