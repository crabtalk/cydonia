//! Links to cydonia's own entries, `cydonia://<project>#<number>` — see
//! [`artifact::reference`] — painted by cydonia rather than as web links.
//!
//! A link card is the entry itself: a session is its turns, read only, whole
//! or the run a reference names, `#43:5-7`; part of an article, `#12:5-7` or
//! `#12#setup`, is those blocks, read only; a whole article or a board is its
//! title, kind and number. A bookmark-sized card of any of them is that one
//! row.
//!
//! The link names the entry; deleting the block leaves the entry where it was.
//!
//! [`Links`] is what cydonia does with every link a document paints: these,
//! files and web pages.

pub mod mention;

use std::{path::PathBuf, rc::Rc};

use crate::{
    model::workspace::{
        Showing,
        references::{Part, Passage},
    },
    view::{
        component::{
            composer::{Composer, ComposerEvent},
            transcript,
        },
        detail::{Pointing, composer_agents},
        kit::{
            icon::kind_icon,
            text::{ShownPath as _, untitled},
        },
        modals::palette::Linked,
        sidebar::Row,
        window::root::Cydonia,
    },
};
use artifact::{kind::Kind, project::Project as _, reference::Span};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, MouseButton, SharedString, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, popover, scroll, surface,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons},
    },
};
use editor::{SlashAction, SlashAt, SlashItem, SlashRow};
use markdown::{
    AppExt as _, BlockKind, Form, Link,
    render::{Editing, fence_band, fence_panel},
};

pub(crate) use artifact::reference::SCHEME;
/// The transcript's box. It scrolls inside; the article does not grow with
/// the conversation.
const TRANSCRIPT_HEIGHT: f32 = 320.;
/// What the transcript lays its column out against.
pub(crate) const CARD_WIDTH: f32 = 720.;
/// How many of a session's last turns the link picker previews.
pub(crate) const PREVIEW_TURNS: usize = 3;
/// How many of an article's first blocks its hover card shows.
const HOVER_BLOCKS: usize = 6;
/// The tallest a hover card's content stands; past it the content is cut.
const HOVER_HEIGHT: f32 = 240.;
/// How wide a hover card stands.
const HOVER_WIDTH: f32 = 320.;

/// The card a link is written into.
fn embed(url: &str) -> BlockKind {
    BlockKind::Bookmark {
        url: url.to_owned(),
        form: Form::Embed(None),
    }
}

/// What cydonia does with the links a document paints. Installed at boot.
pub(crate) struct Links;

impl markdown::Links for Links {
    /// A web link in the system browser, a file in its default application,
    /// a directory in the file manager, and a missing file nowhere. With
    /// shift held, a web link opens in a panel browser tab and a file in a
    /// panel file tab, where the window can take one.
    fn open(link: Link<'_>, window: &mut Window, cx: &mut App) {
        let Some(Some(root)) = window.root::<Cydonia>() else {
            cx.open_url(link.url);
            return;
        };
        if let Some(reference) = link.url.strip_prefix(SCHEME) {
            root.update(cx, |root, cx| root.open_reference(reference, window, cx));
            return;
        }
        let shift = window.modifiers().shift;
        if let Some((path, line)) = file_of(link, &root, cx) {
            // A missing file is neither revealed nor opened; its hover says so.
            if !path.exists() {
                return;
            }
            let opened = shift
                && root.update(cx, |root, cx| {
                    root.open_file_in_panel(path.clone(), line, window, cx)
                });
            if !opened {
                root.update(cx, |root, cx| root.launch_path(path, window, cx));
            }
            return;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let web = link.url.starts_with("https://") || link.url.starts_with("http://");
            if web
                && shift
                && root.update(cx, |root, cx| {
                    root.open_in_panel(link.url.to_owned(), window, cx)
                })
            {
                return;
            }
        }
        cx.open_url(link.url);
    }

    /// Its own links, then web pages. Without the `desktop` feature web
    /// pages are not fetched.
    fn preview(link: Link<'_>, cx: &App) -> Option<markdown::Preview> {
        if link.url.starts_with(SCHEME) {
            return mention::preview(link.url, cx);
        }
        #[cfg(feature = "desktop")]
        return crate::model::link::preview(link.url);
        #[cfg(not(feature = "desktop"))]
        None
    }

    /// Its own links, and nothing else. Inside a session drawn nested — see
    /// [`nested`] — every one of them is a row, drawn without the root.
    fn card(link: Link<'_>, form: Form, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let reference = link.url.strip_prefix(SCHEME)?.to_owned();
        if cx.try_global::<Nested>().is_some_and(|nested| nested.0 > 0) {
            return Some(nested_row(link.url, cx));
        }
        let root = window.root::<Cydonia>().flatten()?;
        Some(root.update(cx, |root, cx| root.entry_card(&reference, form, window, cx)))
    }

    /// Its own links as their entries, a web page as its bookmark, a
    /// picture as itself, any other file or folder as its path, and a missing
    /// one as not found.
    fn hover(link: Link<'_>, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        if link.is_web() {
            return Some(markdown::render::hover_bookmark(link, cx));
        }
        let root = window.root::<Cydonia>().flatten()?;
        if let Some(reference) = link.url.strip_prefix(SCHEME) {
            let reference = reference.to_owned();
            let card = root.update(cx, |root, cx| root.entry_hover(&reference, window, cx));
            return Some(div().w(px(HOVER_WIDTH)).child(card).into_any_element());
        }
        let (path, line) = file_of(link, &root, cx)?;
        let shown = match line {
            Some(line) => format!("{}:{line}", path.shown()),
            None => path.shown(),
        };
        if !path.exists() {
            return Some(Tooltip::card(format!("Not found · {shown}"), None, cx));
        }
        match path.is_file() && markdown::is_image(&path.to_string_lossy()) {
            true => Some(picture_hover(path, shown, cx)),
            false => Some(Tooltip::card(shown, None, cx)),
        }
    }
}

/// The file a link names, as a click resolves it: a relative path in an
/// article or a card is under its project's directory, and elsewhere under
/// the link's base.
fn file_of(
    link: Link<'_>,
    root: &gpui::Entity<Cydonia>,
    cx: &App,
) -> Option<(PathBuf, Option<usize>)> {
    let project = link.base.and_then(|base| {
        root.read(cx)
            .workspace
            .read(cx)
            .projects
            .iter()
            .find(|project| project.store().files(base))
            .map(|project| project.path.clone())
    });
    crate::model::file_url::target(project.as_deref().or(link.base), link.url)
}

/// A picture link's hover card: the picture, and its path under it.
fn picture_hover(path: PathBuf, shown: String, cx: &App) -> AnyElement {
    let theme = Theme::of(cx).clone();
    surface::popover(
        Theme::surface_radius(),
        popover::popover_card_flush(&theme)
            .w(px(HOVER_WIDTH))
            .flex()
            .flex_col()
            .child(
                gpui::img(path)
                    .w_full()
                    .max_h(px(HOVER_HEIGHT))
                    .object_fit(gpui::ObjectFit::Contain),
            )
            .child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(shown),
            ),
    )
    .into_any_element()
}

/// How deep the drawing is inside sessions drawn in another entry or a
/// drawer.
#[derive(Default)]
struct Nested(usize);

impl gpui::Global for Nested {}

/// Draw with [`Links::card`] answering rows, as inside a session drawn
/// nested.
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
    let preview = crate::view::kit::link::mention::preview(url, cx);
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
        .on_click(move |_, window, cx| {
            <Links as markdown::Links>::open(Link::new(&url, None), window, cx)
        })
        .into_any_element()
}

/// The slash menu: the editor's blocks, then a session already running.
pub(crate) fn slash_items() -> Vec<SlashItem> {
    let existing = |at: SlashAt, window: &mut Window, cx: &mut App| {
        let Some(root) = window.root::<Cydonia>().flatten() else {
            return;
        };
        root.update(cx, |root, cx| {
            root.link_search(
                Rc::new(move |_, linked: Linked, _, cx| {
                    at.editor
                        .update(cx, |editor, cx| {
                            editor.place_block(at.block, embed(&linked.url), cx)
                        })
                        .ok();
                }),
                window,
                cx,
            )
        });
    };
    let mut items = editor::slash_defaults();
    items.push(SlashItem::Row(SlashRow {
        label: "Session".into(),
        icon: Some(kind_icon(Kind::Session)),
        action: SlashAction::Run(Rc::new(existing)),
    }));
    items
}

/// An entry a reference resolved to.
pub(crate) struct Named {
    pub(crate) row: Row,
    pub(crate) kind: Kind,
    pub(crate) number: u64,
    pub(crate) part: Option<Part>,
}

impl Cydonia {
    /// The entry a reference names, or why it names none.
    pub(crate) fn named(&self, text: &str, cx: &App) -> Result<Named, String> {
        let resolved = self.workspace.read(cx).resolve(text, cx)?;
        Ok(Named {
            row: Row::Entry {
                project: resolved.project,
                showing: resolved.showing,
            },
            kind: resolved.kind,
            number: resolved.number,
            part: resolved.part,
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
        // A height the card's link states is the whole card's.
        let mut height = None;
        let body = match self.named(reference, cx) {
            Ok(Named {
                row:
                    Row::Entry {
                        showing: Showing::Session(id),
                        ..
                    },
                part,
                number,
                ..
            }) if let Form::Embed(stated) = form => {
                height = stated;
                match part {
                    Some(Part::Turns(turns)) => {
                        self.session_excerpt(id, number, turns, stated, window, cx)
                    }
                    _ => self.session_whole(id, number, stated, window, cx),
                }
            }
            Ok(Named {
                row,
                part: Some(Part::Passage(passage)),
                number,
                ..
            }) if let Form::Embed(stated) = form => {
                height = stated;
                self.article_excerpt(row, number, passage, stated, window, cx)
            }
            Ok(Named {
                row,
                part: Some(Part::Card(card)),
                ..
            }) => row
                .located(self.workspace.read(cx))
                .and_then(|(project, at)| {
                    self.card_embed(project, at, &card.id, reference, window, cx)
                })
                .unwrap_or_else(|| div().into_any_element()),
            Ok(named) => self.entry_row(named, cx),
            Err(why) => div()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(why)
                .into_any_element(),
        };
        fence_panel(&theme)
            .id(SharedString::from(format!("entry-card-{reference}")))
            .w_full()
            .flex()
            .flex_col()
            .when_some(height, |el, height| el.h(px(height as f32)))
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
        let title = named.row.label(self.workspace.read(cx), cx);
        let kind = match named.kind {
            Kind::Session => "Session",
            Kind::Article => "Article",
            Kind::Board => "Board",
            Kind::Table => "Table",
            Kind::App => "App",
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
                .map(|held| {
                    held.board
                        .read(cx)
                        .columns
                        .iter()
                        .map(|column| format!("{} {}", column.name, column.cards.len()))
                        .collect::<Vec<_>>()
                        .join(" · ")
                }),
            _ => None,
        };
        let row = named.row.clone();
        let block = match &named.part {
            Some(Part::Passage(passage)) => Some(passage.blocks.start),
            _ => None,
        };
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
            .on_click(cx.listener(move |this, _, window, cx| match block {
                Some(block) => this.open_passage(&row, block, window, cx),
                None => this.open_row(&row, window, cx),
            }))
            .into_any_element()
    }

    /// Open the article `row` names with block `block` at the top of its pane.
    pub(crate) fn open_passage(
        &mut self,
        row: &Row,
        block: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_row(row, window, cx);
        let editor = self.pane_editor(cx);
        if let Some(editor) = editor {
            let at = markdown::Selection::at(markdown::Cursor::new(block, markdown::Part::Body, 0));
            editor.update(cx, |editor, cx| editor.select_to_top(at, cx));
        }
    }

    /// Part of an article, read only, the way [`Self::session_excerpt`] embeds
    /// a run of turns: a header naming the article and the part, which opens
    /// the article there, over the blocks in a box that scrolls past
    /// [`TRANSCRIPT_HEIGHT`].
    fn article_excerpt(
        &mut self,
        row: Row,
        number: u64,
        passage: Passage,
        height: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = self.passage_body(
            &row,
            &passage,
            height.is_none().then(|| px(TRANSCRIPT_HEIGHT)),
            window,
            cx,
        );
        let header = self.article_header(row, number, &passage, cx);
        div()
            .flex()
            .flex_col()
            .when(height.is_some(), |el| el.flex_1().min_h_0())
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// An article excerpt's band: its mark, title, number and what part of it
    /// this is. Pressing it opens the article at the passage.
    fn article_header(
        &self,
        row: Row,
        number: u64,
        passage: &Passage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let title = row.label(self.workspace.read(cx), cx);
        let block = passage.blocks.start;
        let named = match passage.label.as_str() {
            "" => format!("#{number}"),
            label => format!("#{number} · {label}"),
        };
        fence_band(&theme)
            .id(SharedString::from(format!(
                "article-excerpt-head-{number}-{}",
                passage.source.start
            )))
            .gap(px(8.))
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(
                icons::icon(kind_icon(Kind::Article))
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
                    .child(title),
            )
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_faint)
                    .child(named),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.open_passage(&row, block, window, cx)),
            )
            .into_any_element()
    }

    /// What hovering a link to `reference` shows: an article's opening blocks
    /// or the passage it names, a card's text, and otherwise the entry's row.
    fn entry_hover(
        &mut self,
        reference: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let body = match self.named(reference, cx) {
            Ok(Named {
                row,
                part: Some(Part::Card(card)),
                ..
            }) => row
                .located(self.workspace.read(cx))
                .and_then(|(project, at)| {
                    self.card_embed(project, at, &card.id, reference, window, cx)
                })
                .unwrap_or_else(|| div().into_any_element()),
            Ok(Named {
                row,
                part: Some(Part::Passage(passage)),
                number,
                ..
            }) => self.article_hover(row, number, passage, window, cx),
            Ok(Named {
                row,
                kind: Kind::Article,
                number,
                part: None,
            }) => match self.opening(&row, cx) {
                Some(passage) => self.article_hover(row, number, passage, window, cx),
                None => self.entry_row(
                    Named {
                        row,
                        kind: Kind::Article,
                        number,
                        part: None,
                    },
                    cx,
                ),
            },
            Ok(named) => self.entry_row(named, cx),
            Err(why) => div()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(why)
                .into_any_element(),
        };
        surface::popover(
            Theme::surface_radius(),
            popover::popover_card_flush(&theme)
                .w_full()
                .flex()
                .flex_col()
                .child(body),
        )
        .into_any_element()
    }

    fn article_hover(
        &mut self,
        row: Row,
        number: u64,
        passage: Passage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = self.passage_body(&row, &passage, Some(px(HOVER_HEIGHT)), window, cx);
        let header = self.article_header(row, number, &passage, cx);
        div()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// An article's first [`HOVER_BLOCKS`] blocks, as a passage. `None` for an
    /// empty article or one not found.
    fn opening(&self, row: &Row, cx: &App) -> Option<Passage> {
        let (project, at) = row.located(self.workspace.read(cx))?;
        let disk = self.workspace.read(cx).article_in(project, at)?.disk();
        let count = disk.blocks.len().min(HOVER_BLOCKS);
        let end = disk.blocks.get(count.checked_sub(1)?)?.end;
        Some(Passage {
            blocks: 0..count,
            source: 0..end,
            disk,
            label: String::new(),
        })
    }

    /// The blocks of `passage`, read only, in a box that scrolls past
    /// `max_height` where one is given and fills the box it is put in where
    /// none is.
    pub(crate) fn passage_body(
        &self,
        row: &Row,
        passage: &Passage,
        max_height: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let base = row
            .located(self.workspace.read(cx))
            .and_then(|(project, at)| self.workspace.read(cx).article_in(project, at))
            .and_then(|article| article.path.parent().map(std::path::Path::to_path_buf));
        let doc = markdown::parse_with(passage.markdown(), &cx.marks());
        let editing = Editing {
            base: base.as_deref(),
            ..Default::default()
        };
        let rendered = nested(cx, |cx| markdown::render_with(&doc, editing, window, cx));
        let id = SharedString::from(format!("article-excerpt-{}", passage.source.start));
        let rows = div()
            .id(SharedString::from(format!("{id}-rows")))
            .p(px(16.))
            .child(rendered);
        div()
            // The excerpt's wheel stops here, at its ends too, so what is
            // under it does not scroll with it.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(match max_height {
                Some(height) => scroll::Viewport::new(id, rows.max_h(height), gpui::Axis::Vertical),
                None => {
                    scroll::Viewport::new(id, rows.flex_1().min_h_0(), gpui::Axis::Vertical).fill()
                }
            })
            .when(max_height.is_none(), |el| {
                el.flex_1().min_h_0().flex().flex_col()
            })
            .into_any_element()
    }

    /// A session's name, its agent and whether it is working, and the button
    /// that opens it in a pane.
    fn session_header(&self, id: u64, number: u64, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (title, agent, streaming) = self
            .workspace
            .read(cx)
            .session(id, cx)
            .map(|chat| (chat.title.clone(), chat.entry.name.clone(), chat.streaming))
            .unwrap_or_default();
        fence_band(&theme)
            .gap(px(8.))
            .children(session_label(
                &theme,
                &title,
                match streaming {
                    true => format!("{agent} · #{number} · working"),
                    false => format!("{agent} · #{number}"),
                },
            ))
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

    /// A whole session, read only: every turn, in a box that scrolls past
    /// [`TRANSCRIPT_HEIGHT`].
    fn session_whole(
        &mut self,
        id: u64,
        number: u64,
        height: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.session_header(id, number, cx);
        let all = Span {
            from: 1,
            to: u64::MAX,
        };
        let body = self.excerpt_body(
            id,
            all,
            height.is_none().then(|| px(TRANSCRIPT_HEIGHT)),
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .when(height.is_some(), |el| el.flex_1().min_h_0())
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// The transcript `view` draws over its session's footer, filling the box
    /// it is put in. Without `reply` the composer has no field, and shows the
    /// turn's activity alone.
    pub(crate) fn session_transcript(
        &mut self,
        view: gpui::Entity<transcript::SessionView>,
        reply: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = view.read(cx).session;
        let composer = self.session_composer(id, window, cx);
        composer.update(cx, |composer, cx| composer.set_input(reply, cx));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .when(reply, |el| {
                let dropped = composer.clone();
                el.on_drop(move |paths: &gpui::ExternalPaths, _, cx| {
                    dropped.update(cx, |composer, cx| composer.drop_paths(paths, cx));
                })
            })
            .flex()
            .flex_col()
            .child(view.clone())
            .child({
                let height = view.read(cx).state.footer_height.clone();
                crate::view::pane::footer::Band::new(self.workspace.clone(), Some(id)).render(
                    composer,
                    None,
                    Some(height),
                    cx,
                )
            })
            .into_any_element()
    }

    /// A run of a session's turns, read only, the way GitHub embeds a range of
    /// a file's lines: a header naming the session and the range, which
    /// opens the session at its first turn, over the turns in a box that
    /// scrolls past [`TRANSCRIPT_HEIGHT`].
    fn session_excerpt(
        &mut self,
        id: u64,
        number: u64,
        turns: Span,
        height: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let from = turns.from.saturating_sub(1) as usize;
        let to = turns.to as usize;
        let body = self.excerpt_body(
            id,
            turns,
            height.is_none().then(|| px(TRANSCRIPT_HEIGHT)),
            window,
            cx,
        );
        let read = self.workspace.read(cx).session(id, cx).map(|chat| {
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
        let header = fence_band(&theme)
            .id(SharedString::from(format!("session-excerpt-head-{id}")))
            .gap(px(8.))
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(
                icons::icon(kind_icon(Kind::Session))
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .children(session_label(
                &theme,
                &title,
                match note {
                    Some(note) => format!("{agent} · #{number} · {range} · {note}"),
                    None => format!("{agent} · #{number} · {range}"),
                },
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_session(id, window, cx);
                this.reveal_session_turn(id, from, cx);
            }));
        div()
            .flex()
            .flex_col()
            .when(height.is_some(), |el| el.flex_1().min_h_0())
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// Turns `turns` of session `id`, in a box that scrolls past `max_height`
    /// where one is given and fills the box it is put in where none is.
    pub(crate) fn excerpt_body(
        &mut self,
        id: u64,
        turns: Span,
        max_height: Option<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let from = turns.from.saturating_sub(1) as usize;
        let to = turns.to as usize;
        let body = self.session_excerpt_view(id, from, to, window, cx);
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

    /// Turns `from..to` of session `id` as a read-only view of their own, kept
    /// across frames by where they are drawn.
    pub(crate) fn session_excerpt_view(
        &mut self,
        id: u64,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace
            .update(cx, |workspace, cx| workspace.load_history(id, cx));
        let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
        window
            .use_keyed_state(
                SharedString::from(format!("session-excerpt-view-{id}-{from}-{to}")),
                cx,
                move |_, cx| {
                    transcript::SessionView::new(
                        id,
                        None,
                        transcript::Drawn::Excerpt { from, to },
                        CARD_WIDTH,
                        workspace,
                        root,
                        cx,
                    )
                },
            )
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
        let point = {
            let workspace = self.workspace.read(cx);
            Pointing::of(workspace.session(id, cx), &composer_agents(workspace), cx)
        };
        composer.update(cx, |composer, cx| {
            composer.set_tools(false, cx);
            point.apply(composer, cx);
        });
        cx.subscribe_in(
            &composer,
            window,
            move |this, _, event: &ComposerEvent, window, cx| {
                this.composer_event(Some(id), event, window, cx)
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
        let items = slash_items();
        let linkables = crate::view::kit::link::mention::read(workspace, cx);
        let agents = composer_agents(workspace);
        let pointed: Vec<_> = self
            .session_cards
            .iter()
            .map(|(id, composer)| {
                (
                    composer.clone(),
                    Pointing::of(workspace.session(*id, cx), &agents, cx),
                )
            })
            .collect();
        editor::AppExt::set_slash_items(&mut **cx, items);
        cx.set_global(linkables);
        for (composer, point) in pointed {
            composer.update(cx, |composer, cx| point.apply(composer, cx));
        }
    }
}

/// A session band's title, then its `agent · #n` caption.
fn session_label(theme: &Theme, title: &str, caption: String) -> [AnyElement; 2] {
    [
        div()
            .flex_1()
            .min_w_0()
            .text_ellipsis()
            .text_style(TextStyle::Subheadline)
            .text_color(theme.text)
            .child(untitled(title, "Untitled session"))
            .into_any_element(),
        div()
            .flex_none()
            .text_style(TextStyle::Caption)
            .text_color(theme.text_faint)
            .child(caption)
            .into_any_element(),
    ]
}
