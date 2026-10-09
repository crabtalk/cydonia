//! The article pane: one document, and the sidebar row that opens it.

use crate::{
    memory,
    model::article,
    view::{
        component::{
            menu::{self, Menu},
            transcript::{MARK_AWAY, MARK_READING, MARK_VISIBLE},
        },
        leaf::Pane,
        root::{Cydonia, NewArticle},
        sidebar::{self, Row},
    },
};
use artifact::space::Member;
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, CursorStyle, Div, Entity, Focusable as _, KeyBinding,
        MouseButton, ObjectFit, PathPromptOptions, SharedString, Window, actions, canvas, div, img,
        prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{ControlSize, Sizing as _, TextStyle, Theme, Typeset, ink},
    ui::{
        icons,
        input::TextField,
        menu::Item,
        popover,
        widgets::{ButtonStyle, Buttons as _, Status as _},
    },
};
use editor::AppExt as _;
use editor::Mode;
use markdown::AppExt as _;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

actions!(
    cydonia_article,
    [LeaveTitle, ToggleOutline, TogglePlainText]
);

/// `enter` and `down` in the title move to the content. Bound on the field's
/// own context, which is the only thing deep enough to beat the field itself.
///
/// [`TogglePlainText`] is not here: it is a command a person may move, so it
/// is bound from [`crate::view::keymap`] with the rest of them. What it does
/// to the editor is the same either way — the View menu carries it, so AppKit
/// takes the chord before the window is offered it and the editor's own `⌘E`,
/// inline code, is never reached. That is why the ribbon's code button
/// advertises no chord; see [`crate::view::component::ribbon::keystroke`].
pub fn bindings() -> Vec<KeyBinding> {
    let ctx = Some(article::TITLE_CONTEXT);
    vec![
        KeyBinding::new("enter", LeaveTitle, ctx),
        KeyBinding::new("down", LeaveTitle, ctx),
    ]
}

/// The column the document is set in, matching the transcript's. Off, for a
/// page that reads wide under [`article::Article::wide`], the pane's own width
/// is the measure — see [`column`].
const CONTENT_MAX_WIDTH: f32 = 720.;

/// How tall the cover band is, with a picture in it or without: half the 5:2 a
/// cover is cut at, taken at the column's width. The picture is centred in the
/// band, so what shows is the middle of it.
///
/// Fixed, and not re-derived from the measure a wide page is set to: the band
/// is the same height either way, so setting a page across the pane widens the
/// picture without moving a line of the document under it.
const COVER_HEIGHT: f32 = CONTENT_MAX_WIDTH / 5.;

/// How much empty page hangs below the last line, so the end of a document
/// scrolls clear of the bottom edge of the pane.
const TAIL: f32 = 120.;

/// How far the page holds its text off its own edge, in both measures: a page
/// set across the pane changes where the text stops, not how far in it starts.
///
/// What the title adds to it is the editor's [`editor::Layout::text_inset`],
/// read at paint like the theme — the editor holds its text that far inside
/// its box so a block's drag handle has somewhere to sit, and the title takes
/// the same measure to line up with the first paragraph. That allowance is the
/// handle's whole room, in either measure.
const COLUMN_INSET: f32 = 24.;

/// The most dashes the outline draws at once. A longer outline shows the run
/// of them around the heading being read — see [`dash_window`].
const OUTLINE_DASHES: usize = 12;

const OUTLINE_DASH_GAP: f32 = 6.;

/// The widest the outline's menu is drawn.
const OUTLINE_MAX_WIDTH: f32 = 280.;

/// The tallest the outline's menu is drawn; its rows scroll past it.
const OUTLINE_MAX_HEIGHT: f32 = 320.;

/// The first dash drawn, so that the one at `at` sits mid-column, held at
/// either end of an outline longer than [`OUTLINE_DASHES`].
fn dash_window(count: usize, at: Option<usize>) -> usize {
    at.unwrap_or(0)
        .saturating_sub(OUTLINE_DASHES / 2)
        .min(count.saturating_sub(OUTLINE_DASHES))
}

/// Where the reader is among the headings at `starts`, from where the blocks
/// painted last frame against the scroll box. `None` while no block has painted
/// inside it.
fn reading_of(
    layouts: &markdown::BlockLayouts,
    starts: &[usize],
    blocks: usize,
    scroll: &gpui::ScrollHandle,
) -> Option<article::Reading> {
    let view = scroll.bounds();
    let mut on = (0..blocks).filter(|&ix| {
        layouts
            .block_bounds(ix)
            .is_some_and(|b| b.bottom() > view.top() && b.top() < view.bottom())
    });
    let first = on.next()?;
    let last = on.next_back().unwrap_or(first);
    // Headings at or before a block: the last of them owns its section.
    let upto = |block: usize| starts.partition_point(|&start| start <= block);
    let at = upto(first).checked_sub(1);
    Some(article::Reading {
        at,
        shown: at.unwrap_or(0)..upto(last),
    })
}

/// An article's headings, in order: each one's block, level and text.
fn headings(editor: &editor::Editor) -> Vec<(usize, u8, String)> {
    editor
        .doc()
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(ix, block)| match &block.kind {
            markdown::BlockKind::Heading { level, text } => Some((ix, *level, text.text.clone())),
            _ => None,
        })
        .collect()
}

/// The outline menu's rows, one per heading, stepped in from the highest level
/// the article uses.
fn outline_items(headings: &[(usize, u8, String)]) -> Vec<Item> {
    let top = headings
        .iter()
        .map(|(_, level, _)| *level)
        .min()
        .unwrap_or(1);
    headings
        .iter()
        .map(|(_, level, text)| {
            Item::action(text.clone())
                .with_icon(heading_icon(*level))
                .indented(usize::from(level - top))
        })
        .collect()
}

/// The mark an outline row carries for its heading's level.
fn heading_icon(level: u8) -> &'static [u8] {
    match level {
        1 => icons::text::Heading1,
        2 => icons::text::Heading2,
        3 => icons::text::Heading3,
        4 => icons::text::Heading4,
        5 => icons::text::Heading5,
        _ => icons::text::Heading6,
    }
}

/// Plain-text styling, resolved against the active theme on every paint.
pub fn source_style(theme: &Theme) -> markdown::SourceStyle {
    markdown::SourceStyle {
        line_numbers: true,
        gutter_min_digits: 1,
        gutter_gap: 1.5,
        gutter_color: Some(theme.text_faint),
    }
}

/// The custom marks an article is read and written with: `==text==` is
/// [`editor::HIGHLIGHT_MARK`].
pub fn marks() -> markdown::Marks {
    markdown::Marks::new().with(editor::HIGHLIGHT_MARK, "==")
}

/// The colour [`mark_paint`] washes a highlight in. Global because a painter
/// is a bare `fn`.
static HIGHLIGHT: std::sync::RwLock<crate::model::settings::Paint> = std::sync::RwLock::new(
    crate::model::settings::Paint::Named(crate::model::settings::Highlight::Yellow),
);

/// Paint every highlight in `color` from the next frame on.
pub fn set_highlight(color: crate::model::settings::Paint) {
    if let Ok(mut held) = HIGHLIGHT.write() {
        *held = color;
    }
}

/// How [`marks`] paint: every highlight in the colour [`set_highlight`] chose.
pub fn mark_paint(name: &str, theme: &Theme) -> Option<markdown::MarkPaint> {
    let color = HIGHLIGHT.read().ok().map(|held| *held)?;
    (name == editor::HIGHLIGHT_MARK).then(|| markdown::MarkPaint {
        background: Some(color.solid(theme)),
        ..Default::default()
    })
}

/// The find wash's colour; unset keeps [`markdown::default_find`].
static SEARCH: std::sync::RwLock<Option<crate::model::settings::Paint>> =
    std::sync::RwLock::new(None);

pub fn set_search(color: Option<crate::model::settings::Paint>) {
    if let Ok(mut held) = SEARCH.write() {
        *held = color;
    }
}

/// How find matches paint: the current match in the colour [`set_search`]
/// chose, the rest at half its opacity.
pub fn find_paint(theme: &Theme) -> (bezel::gpui::Hsla, bezel::gpui::Hsla) {
    let Some(color) = SEARCH.read().ok().and_then(|held| *held) else {
        return markdown::default_find(theme);
    };
    let solid = color.solid(theme);
    (solid.opacity(0.5), solid)
}

fn source_offset(editor: &editor::Editor, cx: &App) -> f32 {
    if editor.mode() != Mode::Source {
        return 0.;
    }
    let style = cx.source_style();
    let base = bezel::theme::base_text_size();
    let limits = cx.editor_text_size();
    let size = ((editor.text_size().unwrap_or(base) + cx.editor_text_size_adjustment())
        .clamp(limits.min, limits.max)
        * 10.)
        .round()
        / 10.;
    let text_size = cx.typography().scaled(size / base).body.size();
    let digits = editor
        .source()
        .split('\n')
        .count()
        .to_string()
        .len()
        .max(style.gutter_min_digits);
    // Cancel Bezel's code padding and full gutter so source text aligns with the title.
    12. + if style.line_numbers {
        (digits as f32 + style.gutter_gap.max(0.)) * text_size
    } else {
        0.
    }
}

/// The box the page is set in: the reading column, or the pane itself. The
/// title and the document both take it, since two boxes made conditional apart
/// drift apart the first time one of them is touched.
fn column(wide: bool) -> Div {
    let band = div().w_full();
    match wide {
        true => band,
        false => band.max_w(px(CONTENT_MAX_WIDTH)),
    }
}

/// The band the editor is set in, under the title.
fn page(editor: &Entity<editor::Editor>, wide: bool, source_offset: f32) -> Div {
    // Its own height, not the box's share of one: a long document overflows
    // and scrolls instead of being squashed and clipped, and `min_h_full` is
    // what leaves the band something to scroll *under* when the document is
    // short.
    div()
        .w_full()
        .flex_none()
        .min_h_full()
        .flex()
        .justify_center()
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, {
            let editor = editor.clone();
            move |event, window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.press(
                        event.position,
                        event.click_count,
                        event.modifiers,
                        window,
                        cx,
                    )
                })
            }
        })
        .child(
            // A page, not a paragraph. The editor's box is only as tall as
            // the document, and a pane of dead space under a one-line note
            // reads as something you cannot type in: the band around it is
            // what makes a click down there or beside it land a caret, and
            // the I-beam is what says so before the click.
            column(wide)
                .px(px(COLUMN_INSET))
                .pt(px(20.))
                .pb(px(TAIL))
                .flex()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .ml(px(-source_offset))
                        .child(editor.clone()),
                ),
        )
}

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    /// The menu's New Article. The sidebar's `+` names a project by the
    /// heading it sits under; the menu bar has only the one in front.
    pub(crate) fn new_article_action(
        &mut self,
        _: &NewArticle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.workspace.read(cx).active else {
            return;
        };
        self.new_article(project, window, cx);
    }

    pub(crate) fn new_article(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_article(cx));
        if let Some(ix) = ix {
            self.open_article(project, ix, window, cx);
        }
    }

    /// Show the article and put the caret in it — the pane is a document and
    /// nothing else, so there is nowhere else the focus could sensibly go.
    pub(crate) fn open_article(
        &mut self,
        project: usize,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let member = self.workspace.read(cx).article_member(project, ix);
        if self.enter_member(member, window, cx) {
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.open_article(project, ix, cx));
        self.leaf_mut().pane = Pane::Article;

        let (field, editor, unnamed) = {
            let article = self.pane_doc(cx);
            (
                article.and_then(|article| article.field.clone()),
                article.and_then(|article| article.editor.clone()),
                article.is_some_and(|article| article.title.is_empty()),
            )
        };
        // A page with no name is asking to be given one; a named one is asking
        // to be written in.
        match (unnamed, field, editor) {
            (true, Some(field), _) => window.focus(&field.focus_handle(cx), cx),
            (_, _, Some(editor)) => window.focus(&editor.focus_handle(cx), cx),
            _ => {}
        }
        self.reveal_applied_match(cx);
        cx.notify();
    }

    /// The article the focused pane is on. A window with no space open has no
    /// member to name, and falls back to what its project is pointed at.
    pub(crate) fn pane_doc<'a>(&self, cx: &'a App) -> Option<&'a crate::model::article::Article> {
        self.workspace
            .read(cx)
            .article_of(self.leaf().entry.as_ref())
    }

    /// The same by its file — the one address every write to an article is
    /// made through.
    pub(crate) fn pane_article(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.pane_doc(cx).map(|article| article.path.clone())
    }

    /// Out of the title and into the document under it.
    fn leave_title(&mut self, _: &LeaveTitle, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.pane_doc(cx).and_then(|article| article.editor.clone());
        if let Some(editor) = editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
    }

    /// Swap the document for the markdown it spells, and back — the header
    /// menu's Plain text and its ⌘E.
    ///
    /// The focus goes to the document afterwards, unless the find field has it.
    pub(crate) fn toggle_plain_text(
        &mut self,
        _: &TogglePlainText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mode) = self.plain_text(cx) else {
            return;
        };
        let mode = match mode {
            true => Mode::Blocks,
            false => Mode::Source,
        };
        let Some(on) = self.pane_article(cx) else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            workspace.set_article_mode(&on, mode, cx)
        });
        let finding = self.leaf().finding
            && self
                .leaf()
                .find_field
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
        let editor = self.pane_doc(cx).and_then(|article| article.editor.clone());
        if let Some(editor) = editor.filter(|_| !finding) {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Open or shut the focused article's outline — the View menu's Outline.
    pub(crate) fn toggle_outline(
        &mut self,
        _: &ToggleOutline,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(article) = self.pane_doc(cx) else {
            return;
        };
        let path = article.path.clone();
        let at = article.reading.borrow().at;
        let items = article
            .editor
            .as_ref()
            .map(|editor| outline_items(&headings(editor.read(cx))))
            .unwrap_or_default();
        self.toggle_outline_menu(Menu::Outline(path), at, &items, cx);
    }

    /// Open or shut an outline's menu, opening it lit on the heading being
    /// read.
    fn toggle_outline_menu(
        &mut self,
        menu: Menu,
        at: Option<usize>,
        items: &[Item],
        cx: &mut Context<Self>,
    ) {
        self.toggle_menu(menu.clone(), cx);
        self.outline_hovered = false;
        if self.menu.as_ref() == Some(&menu) {
            self.light_outline(at, items);
        }
    }

    /// Light the outline menu's row for the heading at `at`, or none.
    fn light_outline(&mut self, at: Option<usize>, items: &[Item]) {
        match at {
            Some(at) => {
                self.menu_cursor.point_at(items, &[at]);
            }
            None => self.menu_cursor.clear(),
        }
    }

    /// Whether the open document is being edited as markdown, or `None` where
    /// there is no document to be in either form.
    pub(crate) fn plain_text(&self, cx: &App) -> Option<bool> {
        let article = self.pane_doc(cx)?;
        Some(article.mode(cx) == Mode::Source)
    }

    /// Set the open page across the pane, or back in the reading column — the
    /// header menu's Full width, and `None` for its Use default width. The
    /// open one, since that is the page the menu was asked from.
    pub(crate) fn set_full_width(&mut self, wide: Option<bool>, cx: &mut Context<Self>) {
        let Some(on) = self.pane_article(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.set_full_width(&on, wide, cx));
        cx.notify();
    }

    /// Cut the open article a new cover. Adding the first one comes through
    /// here too — there is nothing to choose between, only a picture to get.
    pub(crate) fn shuffle_cover(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.pane_article(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.shuffle_cover(&on, cx));
        cx.notify();
    }

    /// Swap the generated picture for one of the person's own.
    fn pick_cover(&mut self, cx: &mut Context<Self>) {
        // Taken before the dialog goes up: the page the cover was asked for is
        // the page it lands on, whatever the window is showing by the time a
        // file comes back.
        let Some(on) = self.pane_article(cx) else {
            return;
        };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_cover(&on, Some(&path), cx)
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn remove_cover(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.pane_article(cx) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.set_cover(&on, None, cx));
        cx.notify();
    }

    /// Take the file over the buffer — the notice's Reload.
    fn revert_article(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.revert_article(path, cx));
        self.follow_article(window, cx);
        cx.notify();
    }

    /// Put the caret back in the open document after a re-read replaced it.
    /// The editor is a new entity, so whatever focus the old one held went with
    /// it — and focus on an element no frame draws is focus nowhere.
    ///
    /// Only when the focus is nowhere, or already back on the window: a re-read
    /// that rebuilt nothing leaves the title, a card's composer or whatever
    /// else holds the focus where it is.
    pub(crate) fn follow_article(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.showing(cx) != Some(Pane::Article) {
            return;
        }
        let lost = window
            .focused(cx)
            .is_none_or(|focused| focused == self.focus);
        let editor = self.pane_doc(cx).and_then(|article| article.editor.clone());
        if let Some(editor) = editor.filter(|_| lost) {
            window.focus(&editor.focus_handle(cx), cx);
        }
    }

    /// Keep the buffer and write it over the file — the notice's other answer.
    fn keep_article(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.keep_article(path, cx));
        cx.notify();
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The document. Same frame as [`Cydonia::board`]: the body of the content
    /// card, with the composer stack still pinned under it.
    pub(crate) fn article(
        &self,
        project: usize,
        at: usize,
        on: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self
            .workspace
            .read(cx)
            .article_in(project, at)?
            .editor
            .clone()?;
        self.paint_article_find(&editor, on, cx);
        let article = self.workspace.read(cx).article_in(project, at)?;
        let field = article.field.clone()?;
        let editor = article.editor.clone()?;
        let cover = article.cover.clone();
        let wide = article.wide(self.workspace.read(cx).wide_pages);
        let source_offset = source_offset(editor.read(cx), cx);
        let stale = article.stale.then(|| article.path.clone());
        let path = article.path.clone();
        let reading = article.reading.clone();
        let scroll = article.scroll.clone();
        let document = div()
            .id("article")
            .on_action(cx.listener(Self::leave_title))
            // What the ribbon reads to keep out of a drag — see
            // [`crate::view::component::ribbon`]. Taken in the capture phase
            // and on the way out as well as in, so a release past the pane's
            // edge still ends the gesture.
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| this.set_selecting(true, cx)))
            .capture_any_mouse_up(cx.listener(|this, _, _, cx| this.set_selecting(false, cx)))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.set_selecting(false, cx)),
            )
            .flex_1()
            .min_h_0()
            .w_full()
            .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
            .track_scroll(&article.scroll)
            .flex()
            .flex_col()
            .child(self.header(cover, field, wide, cx))
            .child(page(&editor, wide, source_offset));
        Some(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                // The editor paints its selection only while focused, and a
                // press on chrome with no focus of its own leaves focus where
                // it was.
                .on_mouse_down_out({
                    let editor = editor.clone();
                    move |_, window, cx| {
                        if editor.focus_handle(cx).is_focused(window) {
                            window.blur(cx);
                        }
                    }
                })
                // Above the scroll box rather than inside it: a document long
                // enough to scroll would carry the notice off the top of the
                // pane, and it is about the document as a whole.
                .children(stale.map(|path| self.stale_notice(path, cx)))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(document)
                        .child(scrollbars::Overlay::new(
                            "article-bar",
                            &article.scroll,
                            bezel::gpui::Axis::Vertical,
                        ))
                        .children(self.search_pill(on, cx))
                        .children(self.outline(&editor, &path, reading, scroll, window, cx)),
                )
                // Last, and floated over the document from where the
                // selection ends — the bar is chrome the page runs under.
                .children(self.ribbon(on, window, cx))
                .into_any_element(),
        )
    }

    /// The article listed at `at` in `project`, as a drawer draws it: the
    /// document alone, scrolling in the box it is put in. The title is the
    /// drawer's head row.
    pub(crate) fn article_peek(
        &self,
        project: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let article = self.workspace.read(cx).article_in(project, at)?;
        let editor = article.editor.clone()?;
        let wide = article.wide(self.workspace.read(cx).wide_pages);
        let scroll = article.scroll.clone();
        let source_offset = source_offset(editor.read(cx), cx);
        Some(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("article-peek")
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
                        .track_scroll(&scroll)
                        .flex()
                        .flex_col()
                        .child(page(&editor, wide, source_offset)),
                )
                .child(scrollbars::Overlay::new(
                    "article-peek-bar",
                    &scroll,
                    bezel::gpui::Axis::Vertical,
                ))
                .into_any_element(),
        )
    }

    /// Whether a pane is drawing the article at `path`.
    pub(crate) fn article_on_screen(&self, path: &Path, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        self.leaves.iter().any(|leaf| {
            leaf.pane == Pane::Article
                && workspace
                    .article_of(leaf.entry.as_ref())
                    .is_some_and(|article| article.path == path)
        })
    }

    /// The outline over the pane's bottom right: a dash per heading, longer
    /// the higher the heading, and above it while open the menu of headings.
    /// Picking one puts the caret at its start.
    ///
    /// Nothing while the setting is off, in plain text, or for a document with
    /// no headings.
    fn outline(
        &self,
        editor: &Entity<editor::Editor>,
        path: &Path,
        reading: Rc<RefCell<article::Reading>>,
        scroll: gpui::ScrollHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.workspace.read(cx).settings.appearance.outline {
            return None;
        }
        let read = editor.read(cx);
        if read.mode() == Mode::Source {
            return None;
        }
        let headings = headings(read);
        let items = outline_items(&headings);
        let top = headings.iter().map(|(_, level, _)| *level).min()?;
        let theme = Theme::of(cx).clone();
        let menu = Menu::Outline(path.to_owned());
        let open = self.menu.as_ref() == Some(&menu);
        let now = reading.borrow().clone();
        let tone = |ordinal: usize| {
            ink(if now.at == Some(ordinal) {
                MARK_READING
            } else if now.shown.contains(&ordinal) {
                MARK_VISIBLE
            } else {
                MARK_AWAY
            })
        };
        // Taken after layout, from where the blocks painted, and drawn from on
        // the frame after — the transcript's rail reads its run the same way.
        let starts: Vec<usize> = headings.iter().map(|(ix, _, _)| *ix).collect();
        let blocks = read.doc().blocks.len();
        let root = cx.entity().downgrade();
        let watch = canvas(
            {
                let editor = editor.clone();
                let reading = reading.clone();
                let menu = menu.clone();
                let items = items.clone();
                move |_, window, cx| {
                    let next = reading_of(editor.read(cx).layouts(), &starts, blocks, &scroll);
                    let Some(next) = next else {
                        return;
                    };
                    if *reading.borrow() == next {
                        return;
                    }
                    let at = next.at;
                    *reading.borrow_mut() = next;
                    // After the frame: the root is not to be updated while it
                    // is being painted.
                    let (root, menu, items) = (root.clone(), menu.clone(), items.clone());
                    window.defer(cx, move |_, cx| {
                        let _ = root.update(cx, |this, cx| {
                            if this.menu.as_ref() == Some(&menu) && !this.outline_hovered {
                                this.light_outline(at, &items);
                                cx.notify();
                            }
                        });
                    });
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute();
        let dashes = div()
            .flex()
            .flex_col()
            .items_end()
            .gap(px(OUTLINE_DASH_GAP))
            .children(
                headings
                    .iter()
                    .enumerate()
                    .skip(dash_window(headings.len(), now.at))
                    .take(OUTLINE_DASHES)
                    .map(|(ordinal, (_, level, _))| {
                        let depth = f32::from(level - top);
                        div()
                            .h(px(2.))
                            .w(px((16. - 4. * depth).max(6.)))
                            .rounded_full()
                            .bg(tone(ordinal))
                    }),
            );
        let card = open.then(|| {
            let rows = headings
                .iter()
                .zip(items.iter().cloned())
                .map(|(&(ix, _, _), item)| {
                    let editor = editor.clone();
                    menu::row(item, move |_, window, cx| {
                        let at = markdown::Selection::at(markdown::Cursor::new(
                            ix,
                            markdown::Part::Body,
                            0,
                        ));
                        editor.update(cx, |editor, cx| editor.select_to_top(at, cx));
                        window.focus(&editor.focus_handle(cx), cx);
                    })
                })
                .collect();
            let id = SharedString::from(format!("outline-card-{}", path.display()));
            let items = items.clone();
            let reading = reading.clone();
            let card = self
                .menu_panel(id.clone(), rows, window, cx)
                .id(SharedString::from(format!("{id}-hover")))
                .max_w(px(OUTLINE_MAX_WIDTH))
                .max_h(px(OUTLINE_MAX_HEIGHT))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.outline_hovered = *hovered;
                    if !hovered {
                        this.light_outline(reading.borrow().at, &items);
                        cx.notify();
                    }
                }));
            popover::anchored_menu_above_end(id, card.into_any_element(), None)
        });
        let trigger = theme
            .ghost(SharedString::from(format!("outline-{}", path.display())))
            .relative()
            .p(px(8.))
            .rounded_md()
            .child(dashes)
            .on_click({
                let menu = menu.clone();
                let at = now.at;
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_outline_menu(menu.clone(), at, &items, cx);
                })
            })
            .children(card);
        Some(
            div()
                .absolute()
                .occlude()
                .bottom(px(16.))
                .right(px(16.))
                .child(watch)
                .child(self.menu_press(trigger, menu, cx))
                .into_any_element(),
        )
    }

    /// The file moved under a document that had edits of its own — see
    /// [`article::Article::adopt`]. Both are somebody's work, so the pane says
    /// so and offers the two ways out rather than picking one.
    fn stale_notice(&self, path: PathBuf, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let chip = |id: &'static str, label: &'static str| {
            theme
                .button(label, ButtonStyle::Ghost, Some(Fade::new(painter, id)))
                .control_size(ControlSize::Small)
                .id(id)
        };
        let keep = path.clone();
        theme
            .warning_strip("This file changed on disk while you were editing it.")
            .mx(px(COLUMN_INSET))
            .flex_none()
            .items_center()
            .child(
                theme
                    .control_group()
                    .ml_auto()
                    .flex_none()
                    .child(chip("article-revert", "Reload").on_click(cx.listener(
                        move |this, _, window, cx| this.revert_article(&path, window, cx),
                    )))
                    .child(
                        chip("article-keep", "Keep mine").on_click(
                            cx.listener(move |this, _, _, cx| this.keep_article(&keep, cx)),
                        ),
                    ),
            )
    }

    /// The page's furniture: its picture, and the name under it. Both belong to
    /// the page rather than to the document, so neither is anything the editor
    /// below knows about.
    fn header(
        &self,
        cover: Option<PathBuf>,
        field: Entity<TextField>,
        wide: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        // The band only where there is a picture in it. A page with no cover
        // starts at its title; `Add cover` is in the band's `···`, which is on
        // screen either way and costs the page no room — see
        // `sidebar::entry_menu`.
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .children(cover.map(|cover| self.cover_band(cover, cx)))
            .child(
                div().w_full().flex().justify_center().child(
                    column(wide)
                        .pl(px(COLUMN_INSET + cx.editor_layout().text_inset))
                        .pr(px(COLUMN_INSET))
                        .pt(px(20.))
                        .child(field),
                ),
            )
    }

    /// The picture across the top of a page that has one.
    fn cover_band(&self, cover: PathBuf, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .group("cover")
            .relative()
            .w_full()
            .flex_none()
            .h(px(COVER_HEIGHT))
            .overflow_hidden()
            .child(
                img(cover)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    // Off gpui's own asset cache, which never lets a decoded
                    // cover go. See [`crate::memory`].
                    .image_cache(&memory::covers(cx)),
            )
            .child(self.cover_controls(cx))
    }

    /// The cover's own controls, kept off the page until the pointer is on it.
    ///
    /// Absolute, so neither showing them nor taking the cover away moves a line
    /// of the document — a row that sat in flow would shunt the first paragraph
    /// down the moment the mouse crossed the pane.
    fn cover_controls(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let chip = |id: &'static str, label: &'static str| {
            theme
                .button(label, ButtonStyle::Ghost, Some(Fade::new(painter, id)))
                .control_size(ControlSize::Small)
                .id(id)
        };

        theme
            .control_group()
            .absolute()
            .bottom(px(10.))
            .right(px(10.))
            .invisible()
            .group_hover("cover", |row| row.visible())
            .child(
                chip("cover-shuffle", "Shuffle")
                    .on_click(cx.listener(|this, _, _, cx| this.shuffle_cover(cx))),
            )
            .child(
                chip("cover-change", "Change")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_cover(cx))),
            )
            .child(
                chip("cover-remove", "Remove")
                    .on_click(cx.listener(|this, _, _, cx| this.remove_cover(cx))),
            )
    }

    /// One article in the sidebar, under the project that holds it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn article_row(
        &self,
        entry: &Row,
        project: usize,
        ix: usize,
        title: String,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let light = self.light_of(entry, cx);
        let selected = light.selected();
        let article = workspace
            .projects
            .get(project)
            .and_then(|open| open.articles.get(ix));
        let archived = article.is_some_and(|article| article.archived);
        let tint = light.tint(archived, &theme);
        let id = SharedString::from(sidebar::key_of(entry));

        sidebar::row(
            id,
            "article-row",
            selected,
            lifted,
            self.indent_of(entry, cx),
            &theme,
        )
        .child(
            icons::icon(icons::files::FileText)
                .size(px(14.))
                .flex_none()
                .text_color(tint),
        )
        // Display-only: the title is written at the head of the page, and
        // this row is never a second field for it.
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Body)
                .text_color(tint)
                .child(title),
        )
        .child(self.archive_button(
            format!("archive-{}", sidebar::key_of(entry)),
            "article-row",
            entry,
            archived,
            window,
            cx,
        ))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_article(project, ix, window, cx);
        }))
    }
}
