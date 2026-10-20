//! The article pane: one document, and the sidebar row that opens it.

pub mod comments;
mod note;
pub mod thread;

use crate::model::workspace::Opening;
use crate::view::kit::menu;
use crate::{
    model::palette::Washes,
    model::{
        article,
        workspace::{Showing, Workspace},
    },
    view::{
        component::{
            file::pictures::Pictures,
            margin::{self, Glide, Margin, Slot},
            transcript::{MARK_AWAY, MARK_READING, MARK_VISIBLE},
        },
        kit::labels::{LabelHost, Labels, Target},
        pane::find::{self, Find, Searchable},
        sidebar::Row,
        window::root::{Cydonia, NewArticle},
    },
};
use artifact::space::Entry;
use bezel::ui::scroll as scrollbars;
use bezel::ui::surface::{self, Surfaced as _};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, CursorStyle, Div, Entity, Focusable as _, KeyBinding,
        MouseButton, ObjectFit, PathPromptOptions, Pixels, SharedString, Subscription, WeakEntity,
        Window, actions, canvas, div, image_cache, img, prelude::*, px, retain_all,
    },
    motion::{AppExt as _, Fade, Painter},
    theme::{ControlSize, Sizing as _, TextStyle, Theme, Typeset, ink},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::TextField,
        menu::Item,
        popover,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _, Status as _},
    },
};
use editor::AppExt as _;
use editor::{EditorEvent, Mode};
use markdown::AppExt as _;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
};

/// The column the document is set in, matching the transcript's. Off, for a
/// page that reads wide under [`article::Article::wide`], the pane's own width
/// is the measure — see [`Band`].
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

/// The widest an article's own menus — the outline's, the comments' — are
/// drawn.
const MENU_MAX_WIDTH: f32 = 280.;

/// The tallest an article's own menus are drawn; their rows scroll past it.
const MENU_MAX_HEIGHT: f32 = 320.;

/// The narrowest column notes are set beside. A pane with no room for one and
/// a note's margin beside it gets marks.
const NOTES_MIN_COLUMN: f32 = 480.;

/// The hover group of the line under the title, which shows its empty labels
/// prompt.
const META_GROUP: &str = "article-meta";

/// How far the editor sits below the top of the band it is set in.
const PAGE_TOP: f32 = 20.;

/// Room inside an open comment's card, around its quote, note and actions.
const CARD_PAD: f32 = 12.;

/// How big a margin mark is, square.
const MARK_SIZE: f32 = 24.;

actions!(
    cydonia_article,
    [LeaveTitle, ToggleOutline, TogglePlainText, AskFollowUp]
);

/// Claimed on an open comment's follow-up field, so `enter` asks.
const FOLLOW_UP_CONTEXT: &str = "CydoniaCommentFollowUp";

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
        KeyBinding::new("enter", AskFollowUp, Some(FOLLOW_UP_CONTEXT)),
    ]
}

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
) -> Option<Reading> {
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
    Some(Reading {
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

/// The outline menu's rows with what each does: put the caret at the start
/// of its heading.
fn outline_rows<V: 'static>(
    editor: &Entity<editor::Editor>,
    cx: &App,
) -> Vec<(Item, menu::Act<V>)> {
    let headings = headings(editor.read(cx));
    headings
        .iter()
        .zip(outline_items(&headings))
        .map(|(&(ix, _, _), item)| {
            let editor = editor.clone();
            menu::row::<V>(item, move |_, window, cx| {
                let at =
                    markdown::Selection::at(markdown::Cursor::new(ix, markdown::Part::Body, 0));
                editor.update(cx, |editor, cx| editor.select_to_top(at, cx));
                window.focus(&editor.focus_handle(cx), cx);
            })
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

/// How [`marks`] paint: every highlight in the [`markdown::highlight_wash`]
/// of [`Washes::highlight`].
pub fn mark_paint(name: &str, theme: &Theme) -> Option<markdown::MarkPaint> {
    let wash = markdown::highlight_wash(Washes::current().highlight.solid(theme), theme);
    (name == editor::HIGHLIGHT_MARK).then(|| markdown::MarkPaint {
        background: Some(wash),
        ..Default::default()
    })
}

/// How find matches paint: the [`markdown::find_washes`] of [`Washes::find`].
pub fn find_paint(theme: &Theme) -> (bezel::gpui::Hsla, bezel::gpui::Hsla) {
    let Some(color) = Washes::current().find else {
        return markdown::default_find(theme);
    };
    markdown::find_washes(color.solid(theme))
}

/// How comments paint: cut from [`Washes::comment`]. A resolved comment paints
/// no wash, and its range is still held.
pub fn comment_paint(theme: &Theme) -> markdown::CommentWashes {
    let mut washes = match Washes::current().comment {
        Some(color) => markdown::CommentWashes::of(color.solid(theme)),
        None => markdown::default_comment(theme),
    };
    washes.resolved = washes.resolved.opacity(0.);
    washes
}

/// The type `editor` sets its page in, at the size the reader has zoomed it to.
fn page_type(editor: &editor::Editor, cx: &App) -> markdown::Typography {
    let base = bezel::theme::base_text_size();
    let limits = cx.editor_text_size();
    let size = ((editor.text_size().unwrap_or(base) + cx.editor_text_size_adjustment())
        .clamp(limits.min, limits.max)
        * 10.)
        .round()
        / 10.;
    cx.typography().scaled(size / base)
}

fn source_offset(editor: &editor::Editor, cx: &App) -> f32 {
    if editor.mode() != Mode::Source {
        return 0.;
    }
    let style = cx.source_style();
    let text_size = page_type(editor, cx).body.size();
    let digits = editor
        .source_lines()
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

/// Where the page is set across the pane: the reading column, or the pane
/// itself, and the least room either side of it. The title, the line under it
/// and the document all take one, since rows laid out apart drift apart the
/// first time one of them is touched.
#[derive(Debug, Clone, Copy)]
struct Band {
    wide: bool,
    /// The least room left of the column: the part of the source gutter its
    /// inset does not hold. A pane too narrow to centre the column with the
    /// gutter in it shifts the column right instead of clipping the line
    /// numbers.
    hang: f32,
    /// The least room right of the column: the comment margin's. The column
    /// moves left to make it, and narrows once it reaches the pane's edge.
    margin: f32,
}

impl Band {
    fn new(wide: bool, source_offset: f32) -> Self {
        Self {
            wide,
            hang: (source_offset - COLUMN_INSET).max(0.),
            margin: 0.,
        }
    }

    /// The band with room for a margin of comments: a note's where `notes` and
    /// `width`, the pane's, holds one beside a column of [`NOTES_MIN_COLUMN`],
    /// and a mark's otherwise. A `width` of zero — the pane before its first
    /// layout — counts as room.
    fn commented(self, notes: bool, width: f32) -> Self {
        let fits = width <= 0. || width >= self.hang + NOTES_MIN_COLUMN + Margin::room(true);
        Self {
            margin: Margin::room(notes && fits),
            ..self
        }
    }

    /// A row across the pane: the column, as `column` sets it, with `beside`
    /// filling the room right of it.
    fn row(self, column: impl FnOnce(Div) -> Div, beside: Option<AnyElement>) -> Div {
        let band = div().w_full().min_w_0();
        let band = match self.wide {
            true => band,
            false => band.max_w(px(CONTENT_MAX_WIDTH)),
        };
        div()
            .w_full()
            .flex()
            .child(div().flex_1().min_w(px(self.hang)))
            .child(column(band))
            .child(div().flex_1().relative().min_w(px(self.margin)).children(
                beside.map(|beside| div().absolute().top_0().left_0().size_full().child(beside)),
            ))
    }
}

/// Called with the comment under a press on the page, or `None` for a press on
/// none.
type OnPress = Rc<dyn Fn(Option<u64>, &mut Window, &mut App)>;

/// The band the editor is set in, under the title, with its margin of
/// comments beside it.
fn page(
    editor: &Entity<editor::Editor>,
    band: Band,
    source_offset: f32,
    margin: Option<AnyElement>,
    on_press: Option<OnPress>,
) -> Div {
    // Its own height, not the box's share of one: a long document overflows
    // and scrolls instead of being squashed and clipped, and `min_h_full` is
    // what leaves the band something to scroll *under* when the document is
    // short.
    band.row(
        // A page, not a paragraph. The editor's box is only as tall as
        // the document, and a pane of dead space under a one-line note
        // reads as something you cannot type in: the band around it is
        // what makes a click down there or beside it land a caret, and
        // the I-beam is what says so before the click.
        |column| {
            column
                .px(px(COLUMN_INSET))
                .pt(px(PAGE_TOP))
                .pb(px(TAIL))
                .flex()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .ml(px(-source_offset))
                        .child(editor.clone()),
                )
        },
        margin,
    )
    .flex_none()
    .min_h_full()
    .cursor(CursorStyle::IBeam)
    .on_mouse_down(MouseButton::Left, {
        let editor = editor.clone();
        move |event, window, cx| {
            if let Some(on_press) = &on_press {
                let hit = editor.read(cx).anchor_at(event.position);
                on_press(hit.map(|id| id.0), window, cx);
            }
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
            self.open_article(project, ix, Opening::Kept, window, cx);
        }
    }

    /// Show the article and put the caret in it — the pane is a document and
    /// nothing else, so there is nowhere else the focus could sensibly go.
    pub(crate) fn open_article(
        &mut self,
        project: usize,
        ix: usize,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let Some(member) = self.workspace.read(cx).article_member(project, ix) else {
            return;
        };
        self.enter_member(member.clone(), opening, window, cx);
        let Some(pane) = self.leaf().cloned() else {
            return;
        };
        let editor = member.into_entry().and_then(|member| {
            let view = pane.update(cx, |pane, cx| pane.article_view(member, cx));
            view.update(cx, |view, cx| view.ensure_editor(cx));
            view.read(cx).editor().cloned()
        });
        let (field, unnamed) = {
            let article = self.pane_doc(cx);
            (
                article.and_then(|article| article.field.clone()),
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

    /// The article the focused tab is on.
    pub(crate) fn pane_doc<'a>(&self, cx: &'a App) -> Option<&'a crate::model::article::Article> {
        let entry = self.focused_entry(cx)?;
        self.workspace.read(cx).article_of(Some(&entry), cx)
    }

    /// The same by its file — the one address every write to an article is
    /// made through.
    pub(crate) fn pane_article(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.pane_doc(cx).map(|article| article.path.clone())
    }

    /// Where the view of `article` in the pane showing it is scrolled to.
    pub(crate) fn article_scroll(&self, article: &Entry, cx: &App) -> Option<gpui::ScrollHandle> {
        self.leaves
            .iter()
            .flat_map(|pane| pane.read(cx).articles.iter())
            .map(|view| view.read(cx))
            .find(|view| view.article == *article)
            .map(|view| view.scroll.clone())
    }

    /// The editor of the article the focused pane shows.
    pub(crate) fn pane_editor(&self, cx: &App) -> Option<Entity<editor::Editor>> {
        self.leaf()?
            .read(cx)
            .article_shown(cx)?
            .read(cx)
            .editor()
            .cloned()
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
        let Some(editor) = self.pane_editor(cx) else {
            return;
        };
        editor.update(cx, |editor, cx| editor.toggle_source(cx));
        let finding = self.leaf().is_some_and(|pane| {
            let pane = pane.read(cx);
            pane.finding && pane.find_field.read(cx).focus_handle(cx).is_focused(window)
        });
        if !finding {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Open or shut the focused article's outline — the View menu's Outline.
    pub(crate) fn toggle_outline(
        &mut self,
        _: &ToggleOutline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.leaf().and_then(|pane| pane.read(cx).article_shown(cx)) {
            view.update(cx, |view, cx| view.toggle_outline_menu(window, cx));
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

    // ── chrome ───────────────────────────────────────────────────

    /// The article listed at `at` in `project`, as `on`'s drawer draws it: the
    /// document alone, scrolling in the box it is put in. The title is the
    /// drawer's head row.
    pub(crate) fn article_peek(
        &mut self,
        project: usize,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let member = self
            .workspace
            .read(cx)
            .article_member(project, at)?
            .into_entry()?;
        let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
        let view = window.use_keyed_state(
            SharedString::from(format!("article-peek-{}", member.id)),
            cx,
            move |_, cx| ArticleView::new(member, workspace, root, cx),
        );
        view.update(cx, |view, cx| view.peek(cx))
    }
}

/// Which of an article view's menus is up.
#[derive(Clone, PartialEq, Eq)]
enum ArticleMenu {
    Outline,
}

/// Which of an article's headings the view is on, counted among the headings
/// from 0: the one whose section holds the first block on screen, and every
/// one whose section has a block on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reading {
    pub(crate) at: Option<usize>,
    pub(crate) shown: std::ops::Range<usize>,
}

/// One pane's view of one article: its own editor over the article's shared
/// document, scrolled where this pane left it, with the outline, the margin of
/// comments and the menus it opens.
pub(crate) struct ArticleView {
    /// The article, as the space names it.
    pub(crate) article: Entry,
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    /// Set while the article holds a document — see [`Self::ensure_editor`].
    editor: Option<Entity<editor::Editor>>,
    /// The `Open with` button over the editor's pictures, held for as long as
    /// the editor is.
    pictures: Option<Entity<Pictures>>,
    /// The scroll box the page scrolls in, shared with the editor so typing
    /// follows the caret down.
    pub(crate) scroll: gpui::ScrollHandle,
    /// Where the outline says the reader is, taken from the last frame.
    reading: Rc<RefCell<Reading>>,
    /// Where the margin's notes are gliding, held across frames.
    margin: Rc<RefCell<Glide>>,
    /// How tall the composer under the page stood last frame — see
    /// [`crate::view::detail::footer`].
    pub(crate) composer: Rc<Cell<Pixels>>,
    menu: ContextMenu<Self, ArticleMenu>,
    /// Whether the pointer is over the outline's menu, which then keeps the
    /// row it is on lit rather than following the reader.
    outline_hovered: bool,
    /// What the pane's find looks for in the document.
    find: Find,
    /// The picker the labels under the title open.
    labels: Labels,
    /// The comments button the pane's bar draws for it.
    pub(crate) comments: Entity<comments::CommentsButton>,
    /// The field under the open comment that asks a follow-up on it. One,
    /// since one comment is open at a time.
    follow_up: Entity<TextField>,
    _editor: Option<Subscription>,
}

impl ArticleView {
    pub(crate) fn new(
        article: Entry,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = cx.entity().downgrade();
        let comments = {
            let workspace = workspace.clone();
            cx.new(|cx| comments::CommentsButton::new(workspace, owner, cx))
        };
        let follow_up = cx.new(|cx| {
            TextField::new(cx)
                // The pill is the field's frame.
                .with_frame(false)
                .with_key_context(FOLLOW_UP_CONTEXT)
                .with_placeholder("Ask a follow-up…")
        });
        Self {
            comments,
            follow_up,
            article,
            workspace,
            root,
            editor: None,
            pictures: None,
            scroll: gpui::ScrollHandle::new(),
            reading: Rc::default(),
            margin: Rc::default(),
            composer: Rc::default(),
            menu: ContextMenu::default(),
            outline_hovered: false,
            find: Find::default(),
            labels: Labels::default(),
            _editor: None,
        }
    }

    pub(crate) fn editor(&self) -> Option<&Entity<editor::Editor>> {
        self.editor.as_ref()
    }

    /// Where the article is listed, as `(project, index)`, while it is open.
    fn located(&self, cx: &App) -> Option<(usize, usize)> {
        let workspace = self.workspace.read(cx);
        let project = workspace.project_at(&self.article.project)?;
        let at = workspace.projects[project].article_ix(&self.article.id)?;
        Some((project, at))
    }

    fn model<'a>(&self, cx: &'a App) -> Option<&'a article::Article> {
        let (project, at) = self.located(cx)?;
        self.workspace.read(cx).article_in(project, at)
    }

    /// Put an editor over the article's document, or take it away while the
    /// article holds none; and keep it at the article text size.
    pub(crate) fn ensure_editor(&mut self, cx: &mut Context<Self>) {
        let Some((document, path)) = self
            .model(cx)
            .and_then(|article| Some((article.document.clone()?, article.path.clone())))
        else {
            self.editor = None;
            self.pictures = None;
            self._editor = None;
            return;
        };
        let size = self.workspace.read(cx).article_font_size();
        if let Some(editor) = &self.editor
            && editor.read(cx).document() == &document
        {
            if editor.read(cx).text_size() != Some(size) {
                editor.update(cx, |editor, cx| editor.set_text_size(size, cx));
            }
            return;
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let scroll = self.scroll.clone();
        let (editor, pictures) = crate::view::kit::document::editor(
            &dir,
            document,
            self.workspace.downgrade(),
            |editor| editor.with_text_size(size).with_scroll(scroll),
            cx,
        );
        let subscription = cx.subscribe(&editor, move |this, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::AnchorHovered(over) => {
                    let id = over.map(|id| id.0);
                    this.workspace
                        .update(cx, |workspace, cx| workspace.hover_comment(&path, id, cx));
                }
                // The source gutter is as wide as the line count is long.
                EditorEvent::Changed => cx.notify(),
                _ => {}
            }
        });
        self.editor = Some(editor);
        self.pictures = Some(pictures);
        self._editor = Some(subscription);
    }

    /// Out of the title and into the document under it.
    fn leave_title(&mut self, _: &LeaveTitle, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = &self.editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
    }

    /// Open or shut the outline's menu, opening it lit on the heading being
    /// read.
    pub(crate) fn toggle_outline_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        self.outline_hovered = false;
        ContextMenu::toggle(
            self,
            |this| &mut this.menu,
            ArticleMenu::Outline,
            Place::AboveEnd,
            move |_, _, cx| menu::chooser(outline_rows(&editor, cx)),
            window,
            cx,
        );
        if self.menu.showing(&ArticleMenu::Outline) {
            let at = self.reading.borrow().at;
            self.light_outline(at);
        }
    }

    /// Light the outline menu's row for the heading at `at`, or none.
    fn light_outline(&mut self, at: Option<usize>) {
        match at {
            Some(at) => {
                self.menu.point_at(&[at]);
            }
            None => self.menu.clear_cursor(),
        }
    }

    /// Swap the generated picture for one of the person's own.
    fn pick_cover(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.model(cx).map(|article| article.path.clone()) else {
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
            });
        })
        .detach();
    }

    fn shuffle_cover(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.model(cx).map(|article| article.path.clone()) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.shuffle_cover(&on, cx));
    }

    fn remove_cover(&mut self, cx: &mut Context<Self>) {
        let Some(on) = self.model(cx).map(|article| article.path.clone()) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.set_cover(&on, None, cx));
    }

    /// The article as a drawer draws it: the document alone, scrolling in the
    /// box it is put in.
    fn peek(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.ensure_editor(cx);
        let editor = self.editor.clone()?;
        let wide = self.model(cx)?.wide(self.workspace.read(cx).wide_pages);
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
                        .track_scroll(&self.scroll)
                        .flex()
                        .flex_col()
                        .child(page(
                            &editor,
                            Band::new(wide, source_offset),
                            source_offset,
                            None,
                            None,
                        )),
                )
                .child(scrollbars::Overlay::new(
                    "article-peek-bar",
                    &self.scroll,
                    bezel::gpui::Axis::Vertical,
                ))
                .into_any_element(),
        )
    }

    /// The page: its cover, title, the line under it and the document, with
    /// the margin of comments, the comments button and the outline over it.
    fn body(
        &mut self,
        project: usize,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.editor.clone()?;
        let workspace = self.workspace.read(cx);
        let article = workspace.article_in(project, at)?;
        let field = article.field.clone()?;
        let cover = article.cover.clone();
        let wide = article.wide(workspace.wide_pages);
        let stale = article.stale.then(|| article.path.clone());
        let path = article.path.clone();
        // What the composer under the article covers of the pane's foot.
        let covered =
            match workspace.settings.features.threads() && workspace.thread_reachable(&path, cx) {
                true => f32::from(self.composer.get()) + crate::view::window::root::COMPOSER_BOTTOM,
                false => 0.,
            };
        let notes = article.comments_shown(workspace.settings.appearance.comments)
            == crate::model::settings::CommentDisplay::Full;
        let source_offset = source_offset(editor.read(cx), cx);
        let scroll = self.scroll.clone();
        let margin = self.margin(project, at, &editor, cx);
        let band = Band::new(wide, source_offset);
        let band = match margin.is_some() {
            true => band.commented(notes, f32::from(scroll.bounds().size.width)),
            false => band,
        };
        // The band is set from the pane's width as it last laid out: a pane
        // since resized across the line between notes and marks is laid out
        // again.
        let watch = margin.is_some().then(|| {
            let scroll = scroll.clone();
            canvas(
                move |_, window, _| {
                    let width = f32::from(scroll.bounds().size.width);
                    if band.commented(notes, width).margin != band.margin {
                        window.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
        });
        // What the ribbon reads to keep out of a drag — see
        // [`crate::view::component::ribbon`]. Taken in the capture phase and on
        // the way out as well as in, so a release past the pane's edge still
        // ends the gesture.
        let selecting = |selecting: bool, root: WeakEntity<Cydonia>| {
            move |_: &gpui::MouseUpEvent, _: &mut Window, cx: &mut App| {
                root.update(cx, |root, cx| root.set_selecting(selecting, cx))
                    .ok();
            }
        };
        let document = div()
            .id("article")
            .on_action(cx.listener(Self::leave_title))
            .capture_any_mouse_down({
                let root = self.root.clone();
                move |_, _, cx| {
                    root.update(cx, |root, cx| root.set_selecting(true, cx))
                        .ok();
                }
            })
            .capture_any_mouse_up(selecting(false, self.root.clone()))
            .on_mouse_up_out(MouseButton::Left, selecting(false, self.root.clone()))
            .flex_1()
            .min_h_0()
            .w_full()
            .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
            .track_scroll(&scroll)
            .flex()
            .flex_col()
            .children(watch)
            .child(self.header(cover, field, band, cx))
            .children(self.meta_line(project, at, band, cx))
            .child(
                page(
                    &editor,
                    band,
                    source_offset,
                    margin,
                    Some(self.on_page_press(path.clone())),
                )
                .pb(px(covered)),
            );
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
                            &scroll,
                            bezel::gpui::Axis::Vertical,
                        ))
                        .children(
                            self.outline(&editor, &path, window, cx)
                                // Held over the composer under the article.
                                .map(|outline| {
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .right_0()
                                        .bottom(px(covered))
                                        .child(outline)
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    /// A press on the page opens the comment under it, and a press on none
    /// closes the one open.
    fn on_page_press(&self, path: PathBuf) -> OnPress {
        let workspace = self.workspace.downgrade();
        Rc::new(move |hit, _, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    let Some(article) = workspace.article_at_mut(&path) else {
                        return;
                    };
                    let open = article.active_comment;
                    // A resolved comment's text is washed but has no note.
                    let hit = hit.filter(|id| {
                        article
                            .comments
                            .comments
                            .iter()
                            .any(|comment| comment.id == *id && comment.opens())
                    });
                    if hit != open {
                        workspace.activate_comment(&path, hit, cx);
                    }
                })
                .ok();
        })
    }

    /// The comments beside the page: nothing for an article with none open.
    /// Resolved ones are not shown.
    fn margin(
        &self,
        project: usize,
        at: usize,
        editor: &Entity<editor::Editor>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let workspace = self.workspace.read(cx);
        let article = workspace.article_in(project, at)?;
        let shown: Vec<_> = article
            .comments
            .comments
            .iter()
            .filter(|comment| !comment.resolved)
            .cloned()
            .collect();
        if shown.is_empty() {
            return None;
        }
        let notes = editor
            .read(cx)
            .anchor_tops()
            .into_iter()
            .filter(|(id, _)| shown.iter().any(|comment| comment.id == id.0))
            .map(|(id, top)| (id.0, top + px(PAGE_TOP)))
            .collect();
        let notes_wanted = article.comments_shown(workspace.settings.appearance.comments)
            == crate::model::settings::CommentDisplay::Full;
        let line = page_type(editor.read(cx), cx).body.line_height();
        // The open card's quote is level with the line, not its frame.
        let lift = CARD_PAD - (line - TextStyle::Callout.painted_line_height()) / 2.;
        let note = Note {
            comments: Rc::new(shown),
            open: article.active_comment,
            hovered: article.hovered_comment,
            line,
            follow_up: workspace
                .settings
                .features
                .threads()
                .then(|| self.follow_up.clone()),
            path: article.path.clone(),
            root: self.root.clone(),
        };
        let build: margin::Build = Rc::new(move |slot, window, cx| note.build(slot, window, cx));
        Some(
            Margin::new(
                notes,
                article.active_comment,
                px(lift),
                notes_wanted,
                build,
                self.margin.clone(),
                cx.reduced_motion(),
            )
            .into_any_element(),
        )
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
        let top = headings.iter().map(|(_, level, _)| *level).min()?;
        let theme = Theme::of(cx).clone();
        let reading = self.reading.clone();
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
        let view = cx.entity().downgrade();
        let scroll = self.scroll.clone();
        let watch = canvas(
            {
                let editor = editor.clone();
                let reading = reading.clone();
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
                    // After the frame: the view is not to be updated while it
                    // is being painted.
                    let view = view.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            if this.menu.showing(&ArticleMenu::Outline) && !this.outline_hovered {
                                this.light_outline(at);
                                cx.notify();
                            }
                        });
                    });
                    window.request_animation_frame();
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
        let id = SharedString::from(format!("outline-card-{}", path.display()));
        let card = self
            .menu
            .showing(&ArticleMenu::Outline)
            .then(|| {
                self.menu
                    .card(id.clone(), |this| &mut this.menu, window, cx)
            })
            .flatten()
            .map(|card| {
                let reading = reading.clone();
                let card = card
                    .id(SharedString::from(format!("{id}-hover")))
                    .max_w(px(MENU_MAX_WIDTH))
                    .max_h(px(MENU_MAX_HEIGHT))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.outline_hovered = *hovered;
                        if !hovered {
                            this.light_outline(reading.borrow().at);
                            cx.notify();
                        }
                    }));
                self.menu.place(id.clone(), card)
            });
        let trigger = theme
            .ghost(SharedString::from(format!("outline-{}", path.display())))
            .relative()
            .p(px(8.))
            .rounded_md()
            .child(dashes)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_outline_menu(window, cx);
            }))
            .children(card);
        Some(
            div()
                .absolute()
                .occlude()
                .bottom(px(16.))
                .right(px(16.))
                .child(watch)
                .child(ContextMenu::trigger(
                    trigger,
                    |this: &mut Self| &mut this.menu,
                    ArticleMenu::Outline,
                    cx,
                ))
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
                        move |this, _, _, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.revert_article(&path, cx));
                        },
                    )))
                    .child(chip("article-keep", "Keep mine").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.workspace
                                .update(cx, |workspace, cx| workspace.keep_article(&keep, cx));
                        },
                    ))),
            )
    }

    /// The page's furniture: its picture, and the name under it. Both belong to
    /// the page rather than to the document, so neither is anything the editor
    /// below knows about.
    fn header(
        &self,
        cover: Option<PathBuf>,
        field: Entity<TextField>,
        band: Band,
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
            .child(band.row(
                |column| {
                    column
                        .pl(px(COLUMN_INSET + cx.editor_layout().text_inset))
                        .pr(px(COLUMN_INSET))
                        .pt(px(20.))
                        .font_family(Theme::of(cx).font_body.clone())
                        .child(field)
                },
                None,
            ))
    }

    /// The line under the title: the page's labels.
    fn meta_line(
        &self,
        project: usize,
        at: usize,
        band: Band,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let workspace = self.workspace.read(cx);
        let open = workspace.projects.get(project)?;
        let article = open.articles.get(at)?;
        let target = Target::Page(Row::Entry {
            project: open.path.clone(),
            showing: Showing::Article(article.id.clone()),
        });
        let labels = article.labels.clone();
        let chips = div()
            .id("article-labels")
            .relative()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(4.));
        let chips = self.label_chips(chips, target, &labels, META_GROUP, cx);
        Some(
            band.row(
                |column| {
                    column
                        .group(META_GROUP)
                        .pl(px(COLUMN_INSET + cx.editor_layout().text_inset))
                        .pr(px(COLUMN_INSET))
                        .pt(px(8.))
                        .child(chips)
                },
                None,
            )
            .flex_none()
            .into_any_element(),
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
                // Off gpui's own asset cache, which never lets a decoded
                // picture go. This one is the element's: it is released, with
                // its frame and textures, the first frame the band is not
                // drawn.
                image_cache(retain_all(SharedString::from(format!(
                    "cover-{}",
                    cover.display()
                ))))
                .size_full()
                .child(img(cover).size_full().object_fit(ObjectFit::Cover)),
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
}

impl ArticleView {
    /// Every hit of the find in the document, in document order.
    fn find_hits(&self, cx: &App) -> Vec<markdown::Selection> {
        match (&self.editor, self.find.query()) {
            (Some(editor), Some(query)) => find::hits(editor.read(cx).doc(), &query),
            _ => Vec::new(),
        }
    }

    /// Wash the find's hits, the current one set apart.
    fn paint_find(&self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let hits = self.find_hits(cx);
        let current = self.find.current(hits.len()).map(|ix| hits[ix]);
        let washes = find::annotations(&hits, current);
        if editor.read(cx).washes() != washes.as_slice() {
            editor.update(cx, |editor, cx| editor.set_washes(washes, cx));
        }
    }
}

impl Searchable for ArticleView {
    fn set_find(&mut self, find: Find, cx: &mut Context<Self>) {
        self.find = find;
        cx.notify();
    }

    fn hits(&self, cx: &App) -> usize {
        self.find_hits(cx).len()
    }

    fn reveal_hit(&mut self, cx: &mut Context<Self>) {
        let hits = self.find_hits(cx);
        let hit = self.find.current(hits.len()).map(|ix| hits[ix]);
        if let (Some(editor), Some(hit)) = (&self.editor, hit) {
            editor.update(cx, |editor, cx| editor.select(hit, cx));
        }
    }
}

impl LabelHost for ArticleView {
    fn labels(&self) -> &Labels {
        &self.labels
    }

    fn labels_mut(&mut self) -> &mut Labels {
        &mut self.labels
    }

    fn label_workspace(&self) -> &Entity<Workspace> {
        &self.workspace
    }

    fn label_root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for ArticleView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_editor(cx);
        self.paint_find(cx);
        self.located(cx)
            .and_then(|(project, at)| self.body(project, at, window, cx))
            .unwrap_or_else(|| div().flex_1().into_any_element())
    }
}

/// What the margin builds its notes and marks from.
struct Note {
    comments: Rc<Vec<artifact::article::comments::Comment>>,
    open: Option<u64>,
    /// The comment whose note or text the pointer is over.
    hovered: Option<u64>,
    /// How tall a line of the page is.
    line: f32,
    /// The open comment's follow-up field, while article threads are on.
    follow_up: Option<Entity<TextField>>,
    path: PathBuf,
    root: gpui::WeakEntity<Cydonia>,
}

impl Note {
    fn build(&self, slot: Slot, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = Theme::of(cx).clone();
        match slot {
            Slot::Note(id) => {
                let Some(comment) = self.comments.iter().find(|comment| comment.id == id) else {
                    return div().into_any_element();
                };
                // Occluding: the page under it would take the press and put
                // the caret there.
                let slot = div().id(("comment", id)).occlude().w_full();
                // Only the closed row is a button. An open card's text takes a
                // selection, and a press on the page is what closes it.
                let slot = match self.open == Some(id) {
                    true => slot.child(self.card(comment, &theme, window, cx)),
                    false => slot
                        .cursor_pointer()
                        .child(self.closed(comment, &theme))
                        .on_click(self.toggle(id)),
                };
                self.hovered(slot, id).into_any_element()
            }
            Slot::Open(id) => {
                let Some(comment) = self.comments.iter().find(|comment| comment.id == id) else {
                    return div().into_any_element();
                };
                self.hovered(
                    div()
                        .id(("comment-open", id))
                        .occlude()
                        .w_full()
                        .child(self.card(comment, &theme, window, cx)),
                    id,
                )
                .into_any_element()
            }
            Slot::Mark { first, count } => {
                let tint = match self.open == Some(first) || self.hovered == Some(first) {
                    true => comment_paint(&theme).color,
                    false => theme.text_muted,
                };
                div()
                    .w_full()
                    .h(px(self.line))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        theme
                            .ghost(("comment-mark", first))
                            .occlude()
                            .h(px(MARK_SIZE))
                            .min_w(px(MARK_SIZE))
                            .px(px(4.))
                            .justify_center()
                            .gap(px(2.))
                            .child(
                                icons::icon(icons::social::MessageCircle)
                                    .size(px(14.))
                                    .text_color(tint),
                            )
                            .when(count > 1, |el| {
                                el.child(
                                    div()
                                        .text_style(TextStyle::Caption)
                                        .text_color(tint)
                                        .child(count.to_string()),
                                )
                            })
                            .on_click(self.toggle(first)),
                    )
                    .into_any_element()
            }
        }
    }

    /// A closed comment: its mark and the first line of its note, one line of
    /// the page tall.
    fn closed(
        &self,
        comment: &artifact::article::comments::Comment,
        theme: &Theme,
    ) -> impl IntoElement {
        let id = comment.id;
        let lit = self.hovered == Some(id);
        let washes = comment_paint(theme);
        let (tint, text) = match (lit, comment.detached()) {
            (true, _) => (washes.color, theme.text),
            (false, true) => (theme.text_faint, theme.text_faint),
            (false, false) => (theme.text_muted, theme.text_muted),
        };
        theme
            .ghost(("comment-line", id))
            .h(px(self.line))
            .px(px(6.))
            .gap(px(6.))
            // Over the ghost's own hover wash: lit from the page or from
            // here, the row takes its words' colour.
            .when(lit, |el| {
                el.bg(washes.active).hover(move |el| el.bg(washes.active))
            })
            .child(
                icons::icon(icons::social::MessageCircle)
                    .flex_none()
                    .size(px(14.))
                    .text_color(tint),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Callout)
                    .text_color(text)
                    .child(comment.headline().to_owned()),
            )
    }

    /// An open comment: the words it is on, the whole note, and what can be
    /// done with it.
    fn card(
        &self,
        comment: &artifact::article::comments::Comment,
        theme: &Theme,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let id = comment.id;
        // Its own view: selecting in the text answers the pointer there.
        let text = self.root.upgrade().map(|root| {
            let workspace = root.read(cx).workspace.clone();
            let path = self.path.clone();
            window.use_keyed_state(
                SharedString::from(format!("comment-text-{}-{id}", self.path.display())),
                cx,
                move |_, cx| note::NoteText::new(workspace, path, id, cx),
            )
        });
        // The card is contents only; its fill is the surface's.
        popover::popover_card_flush(theme)
            .w_full()
            .p(px(CARD_PAD))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(text)
            .children(self.follow_up_field(comment, theme))
            .child(self.actions(comment, theme))
            .surface(theme, crate::view::component::composer::SURFACE)
    }

    /// The field that asks a follow-up on `comment`: what is typed goes into
    /// its thread and to the article's session, the passage quoted over it.
    fn follow_up_field(
        &self,
        comment: &artifact::article::comments::Comment,
        theme: &Theme,
    ) -> Option<AnyElement> {
        let field = self.follow_up.clone()?;
        let (id, path, root) = (comment.id, self.path.clone(), self.root.clone());
        let quote = comment.quote.clone();
        let ask = {
            let field = field.clone();
            move |_: &AskFollowUp, window: &mut Window, cx: &mut App| {
                let question = field.read(cx).content().trim().to_owned();
                if question.is_empty() {
                    return;
                }
                field.update(cx, |field, cx| field.clear(cx));
                let text = format!(
                    "{}\n\nFollow-up on note {id}: {question}",
                    crate::view::component::composer::blockquote(&quote)
                );
                root.update(cx, |root, cx| {
                    root.workspace.update(cx, |workspace, cx| {
                        workspace.ask_on_comment(&path, id, question, cx);
                        if let Some(session) = workspace.article_session(&path, cx) {
                            workspace.send(session, text, cx);
                        }
                    })
                })
                .ok();
                window.focus(&field.focus_handle(cx), cx);
            }
        };
        // Its own layer over the card's: inside one glass layer every primitive
        // shares one draw order, and the pill's glass would not see past the
        // card's fill.
        Some(
            surface::layered(
                div()
                    .rounded_full()
                    .px(px(12.))
                    .py(px(6.))
                    .text_style(TextStyle::Callout)
                    // The card under it opens and shuts on a press.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_action(ask)
                    .child(field)
                    .surface(theme, crate::view::component::composer::SURFACE),
            )
            .into_any_element(),
        )
    }

    fn actions(&self, comment: &artifact::article::comments::Comment, theme: &Theme) -> Div {
        let id = comment.id;
        let path = self.path.clone();
        let root = self.root.clone();
        let button = |icon, key: &'static str, label: &'static str| {
            theme
                .icon_button(icon, ButtonStyle::Ghost, None)
                .id((key, id))
                .tooltip(move |window, cx| Tooltip::text(label, window, cx))
        };
        div()
            .flex()
            .gap(px(2.))
            .child({
                let (root, path) = (root.clone(), path.clone());
                button(icons::glyph::Check, "comment-resolve", "Resolve").on_click(
                    move |_, _, cx| {
                        root.update(cx, |root, cx| {
                            root.workspace.update(cx, |workspace, cx| {
                                workspace.resolve_comment(&path, id, true, cx)
                            })
                        })
                        .ok();
                    },
                )
            })
            .child(
                button(icons::files::Trash, "comment-delete", "Delete").on_click(
                    move |_, _, cx| {
                        root.update(cx, |root, cx| {
                            root.workspace
                                .update(cx, |workspace, cx| workspace.delete_comment(&path, id, cx))
                        })
                        .ok();
                    },
                ),
            )
    }

    /// Washes its text while the pointer is over it.
    fn hovered(&self, el: gpui::Stateful<Div>, id: u64) -> gpui::Stateful<Div> {
        let (root, path) = (self.root.clone(), self.path.clone());
        el.on_hover(move |hovered, _, cx| {
            let id = hovered.then_some(id);
            root.update(cx, |root, cx| {
                root.workspace
                    .update(cx, |workspace, cx| workspace.hover_comment(&path, id, cx))
            })
            .ok();
        })
    }

    /// Opens the note, or closes it when it is the one open.
    fn toggle(&self, id: u64) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
        let (root, path, open) = (self.root.clone(), self.path.clone(), self.open);
        move |_, _, cx| {
            let id = (open != Some(id)).then_some(id);
            root.update(cx, |root, cx| {
                root.workspace.update(cx, |workspace, cx| {
                    workspace.activate_comment(&path, id, cx)
                })
            })
            .ok();
        }
    }
}
