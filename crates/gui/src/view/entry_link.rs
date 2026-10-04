//! Links to cydonia's own entries, `cydonia://<project>#<number>` — see
//! [`artifact::reference`] — painted by cydonia rather than as web links.
//!
//! A link card is the entry itself: a session whole is live, with a composer
//! that sends to it, and a run of its turns, `#43:5-7`, is those turns alone,
//! read only; an article or a board is its title, kind and number. A
//! bookmark-sized card of any of them is that one row.
//!
//! The link names the entry; deleting the block leaves the entry where it was.

use std::rc::Rc;

use crate::{
    model::workspace::Showing,
    view::{
        component::{
            composer::{Composer, ComposerEvent},
            transcript,
        },
        detail::footer,
        root::Cydonia,
        search::{Linked, kind_icon},
        sidebar::Row,
    },
};
use artifact::{reference::Turns, search::Kind};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Focusable, MouseButton, SharedString, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, scroll,
        widgets::{ButtonStyle, Buttons},
    },
};
use editor::{SlashAction, SlashAt, SlashItem, SlashRow};
use markdown::{BlockKind, Form};

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

/// The card a reference is written into.
fn embed(reference: &str) -> BlockKind {
    BlockKind::Bookmark {
        url: link(reference),
        form: Form::Embed,
    }
}

/// The card renderer cydonia installs at boot: its own links, and nothing
/// else.
///
/// Inside a session drawn nested — see [`nested`] — every one of them is a
/// row, drawn without the root.
pub(crate) fn card(url: &str, form: Form, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let reference = url.strip_prefix(SCHEME)?.to_owned();
    if cx.try_global::<Nested>().is_some_and(|nested| nested.0 > 0) {
        return Some(nested_row(url, cx));
    }
    let root = window.root::<Cydonia>().flatten()?;
    Some(root.update(cx, |root, cx| root.entry_card(&reference, form, window, cx)))
}

/// How deep the drawing is inside sessions drawn in another entry or a
/// drawer.
#[derive(Default)]
struct Nested(usize);

impl gpui::Global for Nested {}

/// Draw with [`card`] answering rows, as inside a session drawn nested.
pub(crate) fn nested<R>(cx: &mut App, draw: impl FnOnce(&mut App) -> R) -> R {
    enter_nested(cx);
    let drawn = draw(cx);
    leave_nested(cx);
    drawn
}

pub(crate) fn enter_nested(cx: &mut App) {
    cx.default_global::<Nested>().0 += 1;
}

pub(crate) fn leave_nested(cx: &mut App) {
    let nested = cx.default_global::<Nested>();
    nested.0 = nested.0.saturating_sub(1);
}

/// A link's row inside a nested session: its title and mark, from what `@`
/// lists. Pressing it opens the link.
fn nested_row(url: &str, cx: &App) -> AnyElement {
    let theme = Theme::of(cx).clone();
    let preview = crate::view::mention::preview(url, cx);
    let title = preview
        .as_ref()
        .and_then(|preview| preview.title.clone())
        .unwrap_or_else(|| SharedString::from(url.to_owned()));
    let glyph = preview.and_then(|preview| preview.glyph);
    let url = url.to_owned();
    div()
        .id(SharedString::from(format!("entry-nested-{url}")))
        .w_full()
        .px(px(12.))
        .py(px(10.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .cursor_pointer()
        .hover(|el| el.bg(theme.element_hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(glyph.map(|glyph| {
            icons::icon(glyph)
                .size(px(16.))
                .text_color(theme.text_muted)
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_style(TextStyle::Callout)
                .text_color(theme.text)
                .child(title),
        )
        .on_click(move |_, window, cx| open_link(&url, window, cx))
        .into_any_element()
}

/// Opens a link clicked in an article or a transcript: an http(s) link in a
/// panel tab where Settings says so and the window can take one, else in the
/// system browser. Installed as markdown's link handler.
pub fn open_link(url: &str, window: &mut Window, cx: &mut App) {
    if let Some(reference) = url.strip_prefix(SCHEME) {
        if let Some(Some(root)) = window.root::<Cydonia>() {
            root.update(cx, |root, cx| root.open_reference(reference, window, cx));
        }
        return;
    }
    #[cfg(not(target_os = "linux"))]
    {
        use crate::model::settings::{Browsing, Links};
        let panel = cx
            .try_global::<Browsing>()
            .is_some_and(|browsing| browsing.links == Links::Panel);
        let web = url.starts_with("https://") || url.starts_with("http://");
        if panel
            && web
            && let Some(Some(root)) = window.root::<Cydonia>()
            && root.update(cx, |root, cx| {
                root.open_in_panel(url.to_owned(), window, cx)
            })
        {
            return;
        }
    }
    cx.open_url(url);
}

/// The slash menu: the editor's blocks, then a new session on each agent,
/// under the agent's mark from the registry, and one already running.
pub(crate) fn slash_items(workspace: &crate::model::workspace::Workspace) -> Vec<SlashItem> {
    let mut rows = workspace
        .settings
        .agents
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
                        Some((id, workspace.reference_of_session(id)?))
                    })
                });
                if let Some((id, reference)) = made {
                    at.editor
                        .update(cx, |editor, cx| {
                            editor.place_block(at.block, embed(&reference), cx)
                        })
                        .ok();
                    root.update(cx, |root, cx| {
                        let composer = root.session_composer(id, window, cx);
                        window.focus(&composer.focus_handle(cx), cx);
                    });
                }
            };
            SlashRow {
                label: SharedString::from(agent.name.clone()),
                icon: workspace.agent_icon(&agent.name),
                action: SlashAction::Run(Rc::new(run)),
            }
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
                            editor.place_block(at.block, embed(&linked.reference), cx)
                        })
                        .ok();
                }),
                window,
                cx,
            )
        });
    };
    rows.push(SlashRow {
        label: "Existing…".into(),
        icon: Some(icons::text::Link.into()),
        action: SlashAction::Run(Rc::new(existing)),
    });
    let mut items = editor::slash_defaults();
    items.push(SlashItem::Group {
        label: "Session".into(),
        icon: Some(kind_icon(Kind::Session)),
        rows,
    });
    items
}

/// The open project a reference names by its directory's name, and the
/// active one for a reference naming none.
/// Read a session's turns back off disk, where they were left unloaded.
pub(crate) fn load_history(workspace: &mut crate::model::workspace::Workspace, id: u64) {
    if let Some(chat) = workspace
        .projects
        .iter_mut()
        .find_map(|project| project.session_mut(id))
    {
        chat.load_history();
    }
}

/// An entry a reference resolved to.
pub(crate) struct Named {
    pub(crate) row: Row,
    pub(crate) kind: Kind,
    pub(crate) number: u64,
    pub(crate) turns: Option<Turns>,
}

impl Cydonia {
    /// The entry a reference names, or why it names none.
    pub(crate) fn named(&self, text: &str, cx: &App) -> Result<Named, String> {
        let resolved = self.workspace.read(cx).resolve(text)?;
        Ok(Named {
            row: Row::Entry {
                project: resolved.project,
                showing: resolved.showing,
            },
            kind: resolved.kind,
            number: resolved.number,
            turns: resolved.turns,
        })
    }

    /// Open the entry a reference names, `project#12` — the target of a
    /// `cydonia://` link — in the drawer of the pane last pressed in.
    pub(crate) fn open_reference(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let on = self.pressed_pane.clone();
        self.peek(on.as_ref(), text, window, cx);
    }

    fn entry_card(
        &mut self,
        reference: &str,
        form: Form,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let body = match self.named(reference, cx) {
            Ok(Named {
                row:
                    Row::Entry {
                        showing: Showing::Session(id),
                        ..
                    },
                turns,
                ..
            }) if form == Form::Embed => match turns {
                None => self.session_live(id, window, cx),
                Some(turns) => self.session_excerpt(id, turns, window, cx),
            },
            Ok(named) => self.entry_row(named, cx),
            Err(why) => div()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(why)
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("entry-card-{reference}")))
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .overflow_hidden()
            // The card's presses are the card's: the editor would otherwise
            // put its caret in the block and show the link instead.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(body)
            .into_any_element()
    }

    /// One row naming an entry: its mark, its title, its kind and number, and
    /// a board's columns. Pressing it opens the entry.
    pub(crate) fn entry_row(&self, named: Named, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let title = self.label_of_row(&named.row, cx);
        let kind = match named.kind {
            Kind::Session => "Session",
            Kind::Article => "Article",
            Kind::Board => "Board",
        };
        let columns = match &named.row {
            Row::Entry {
                project,
                showing: Showing::Board(id),
            } => self
                .workspace
                .read(cx)
                .projects
                .iter()
                .find(|open| &open.path == project)
                .and_then(|open| open.boards.get(open.board_ix(id)?))
                .map(|board| {
                    board
                        .columns
                        .iter()
                        .map(|column| format!("{} {}", column.name, column.cards.len()))
                        .collect::<Vec<_>>()
                        .join(" · ")
                }),
            _ => None,
        };
        let row = named.row.clone();
        div()
            .id("entry-row")
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(
                icons::icon(kind_icon(named.kind))
                    .size(px(16.))
                    .text_color(theme.text_muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_ellipsis()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .children(columns.map(|columns| {
                        div()
                            .text_ellipsis()
                            .text_style(TextStyle::Caption)
                            .text_color(theme.text_faint)
                            .child(columns)
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_faint)
                    .child(format!("{kind} · #{}", named.number)),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_row(&row, window, cx)))
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

    /// A whole session, as its pane draws it: the transcript, and the plan,
    /// any permission asked and a composer that sends to it floating over the
    /// transcript's foot. A picture dropped anywhere on it goes to that
    /// composer.
    fn session_live(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let header = self.session_header(id, cx);
        let transcript = self.session_transcript(id, None, window, cx);
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .h(px(TRANSCRIPT_HEIGHT))
                    .flex()
                    .flex_col()
                    .child(transcript),
            )
            .into_any_element()
    }

    /// Session `id`'s transcript with the composer that sends to it, filling
    /// the box it is put in. Scrolled by `list` where one is given, else by
    /// the session's own.
    pub(crate) fn session_transcript(
        &mut self,
        id: u64,
        list: Option<&bezel::ui::list::VariableList<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let composer = self.session_composer(id, window, cx);
        let (transcript, footer_height) =
            self.workspace
                .update(cx, |workspace, cx| match workspace.session(id) {
                    Some(chat) => (
                        Some(transcript::render(
                            chat,
                            None,
                            CARD_WIDTH,
                            |_, _| None,
                            transcript::Drawn::Nested(list),
                            window,
                            cx,
                        )),
                        Some(chat.transcript.footer_height.clone()),
                    ),
                    None => (None, None),
                });
        let dropped = composer.clone();
        div()
            .id(SharedString::from(format!("session-card-transcript-{id}")))
            .relative()
            .flex_1()
            .min_h_0()
            // The transcript's wheel stops here, at its ends too, so what is
            // under it does not scroll with it.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_drop(move |paths: &gpui::ExternalPaths, _, cx| {
                dropped.update(cx, |composer, cx| composer.drop_paths(paths, cx));
            })
            .flex()
            .flex_col()
            .children(transcript)
            .child(footer(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(self.plan(Some(id), cx))
                    .children(self.permission(Some(id), cx))
                    .child(composer),
                footer_height,
            ))
            .into_any_element()
    }

    /// A run of a session's turns, read only, the way GitHub embeds a range of
    /// a file's lines: a header naming the session and the range, which
    /// opens the session at its first turn, over the turns in a box that
    /// scrolls past [`TRANSCRIPT_HEIGHT`].
    fn session_excerpt(
        &mut self,
        id: u64,
        turns: Turns,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let from = turns.from.saturating_sub(1) as usize;
        let to = turns.to as usize;
        let body = self.excerpt_body(id, turns, Some(px(TRANSCRIPT_HEIGHT)), window, cx);
        let read = self.workspace.read(cx).session(id).map(|chat| {
            (
                chat.title.clone(),
                chat.entry.name.clone(),
                artifact::session::chat::turns(&chat.items).len(),
                chat.streaming,
            )
        });
        let Some((title, agent, count, streaming)) = read else {
            return div().into_any_element();
        };
        let range = match turns.from == turns.to {
            true => format!("Turn {} of {count}", turns.from),
            false => format!("Turns {}–{} of {count}", turns.from, turns.to),
        };
        let note = if to > count {
            Some(format!("Turn {to} doesn’t exist yet"))
        } else if streaming && to == count {
            Some(format!("Turn {to} is still running"))
        } else {
            None
        };
        let header = div()
            .id(SharedString::from(format!("session-excerpt-head-{id}")))
            .h(px(36.))
            .px(px(12.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(
                icons::icon(kind_icon(Kind::Session))
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
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
                    .child(match note {
                        Some(note) => format!("{agent} · {range} · {note}"),
                        None => format!("{agent} · {range}"),
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_session(id, window, cx);
                if let Some(chat) = this.workspace.read(cx).session(id) {
                    chat.transcript.reveal_turn(from);
                }
            }));
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// Turns `turns` of session `id`, in a box that scrolls past `max_height`
    /// where one is given and fills the box it is put in where none is.
    pub(crate) fn excerpt_body(
        &mut self,
        id: u64,
        turns: Turns,
        max_height: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let from = turns.from.saturating_sub(1) as usize;
        let to = turns.to as usize;
        let body = self.workspace.update(cx, |workspace, cx| {
            load_history(workspace, id);
            let chat = workspace.session(id)?;
            Some(transcript::excerpt(chat, from, to, window, cx))
        });
        let Some(body) = body else {
            return div().into_any_element();
        };
        let rows = div()
            .id(SharedString::from(format!("session-excerpt-rows-{id}")))
            .child(body);
        div()
            // The excerpt's wheel stops here, at its ends too, so what is
            // under it does not scroll with it.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(match max_height {
                Some(height) => scroll::Viewport::new(
                    SharedString::from(format!("session-excerpt-{id}")),
                    rows.max_h(height),
                    gpui::Axis::Vertical,
                ),
                None => scroll::Viewport::new(
                    SharedString::from(format!("session-excerpt-{id}")),
                    rows.flex_1().min_h_0(),
                    gpui::Axis::Vertical,
                )
                .fill(),
            })
            .when(max_height.is_none(), |el| {
                el.flex_1().min_h_0().flex().flex_col()
            })
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
        let items = slash_items(workspace);
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
