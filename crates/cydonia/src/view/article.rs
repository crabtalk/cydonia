//! The article pane: one document, and the sidebar row that opens it.

use crate::{
    memory,
    model::{article, workspace::Showing},
    view::{
        leaf::Pane,
        root::{Cydonia, NewArticle},
        sidebar::{self, Row},
    },
};
use artifact::layout::Member;
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, CursorStyle, Div, Entity, Focusable as _, KeyBinding,
        MouseButton, ObjectFit, PathPromptOptions, SharedString, Window, actions, div, img,
        prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{ControlSize, Sizing as _, TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
        widgets::{ButtonStyle, Buttons as _, Status as _},
    },
};
use editor::Mode;
use std::path::{Path, PathBuf};

actions!(cydonia_article, [LeaveTitle, TogglePlainText]);

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

/// The column's own inset. What the title adds to it is the editor's
/// [`editor::Layout::text_inset`], read at paint like the theme — the editor
/// holds its text that far inside its box so a block's drag handle has
/// somewhere to sit, and the title takes the same measure to line up with the
/// first paragraph.
const COLUMN_INSET: f32 = 24.;

/// What a wide page is held off the edge of the pane by. Twice the column's,
/// because the column has white space either side of it standing in for a
/// margin and a page filling the pane has none — at the column's own inset the
/// text runs into the border, and the drag handle has nowhere left to sit.
const WIDE_INSET: f32 = COLUMN_INSET * 2.;

/// Plain-text styling, resolved against the active theme on every paint.
pub fn source_style(theme: &Theme) -> markdown::SourceStyle {
    markdown::SourceStyle {
        line_numbers: true,
        gutter_min_digits: 1,
        gutter_gap: 1.5,
        gutter_color: Some(theme.text_faint),
    }
}

fn source_offset(editor: &editor::Editor, cx: &App) -> f32 {
    if editor.mode() != Mode::Source {
        return 0.;
    }
    let style = markdown::SourceStyle::of(cx);
    let base = bezel::theme::base_text_size();
    let limits = editor::TextSize::of(cx);
    let size = ((editor.text_size().unwrap_or(base) + editor::text_size_adjustment(cx))
        .clamp(limits.min, limits.max)
        * 10.)
        .round()
        / 10.;
    let code_size = markdown::Typography::of(cx).scaled(size / base).code.size();
    let digits = editor
        .source()
        .split('\n')
        .count()
        .to_string()
        .len()
        .max(style.gutter_min_digits);
    // Cancel Bezel's code padding and full gutter so source text aligns with the title.
    12. + if style.line_numbers {
        (digits as f32 + style.gutter_gap.max(0.)) * code_size
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

/// How far that box holds its text off its own edge.
fn inset(wide: bool) -> f32 {
    match wide {
        true => WIDE_INSET,
        false => COLUMN_INSET,
    }
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
        let member = self
            .workspace
            .read(cx)
            .member_of(project, Showing::Article(ix));
        if self.enter_member(member, window, cx) {
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.open_article(project, ix, cx));
        self.leaf_mut().pane = Pane::Article;

        let (field, editor, unnamed) = {
            let article = self.workspace.read(cx).active_article();
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
        cx.notify();
    }

    /// Out of the title and into the document under it.
    fn leave_title(&mut self, _: &LeaveTitle, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self
            .workspace
            .read(cx)
            .active_article()
            .and_then(|article| article.editor.clone());
        if let Some(editor) = editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
    }

    /// Swap the document for the markdown it spells, and back — the header
    /// menu's Plain text and its ⌘E.
    ///
    /// The focus goes back to the document afterwards: the switch carries the
    /// caret across, and a caret in a surface nobody is typing in is a caret
    /// that has to be clicked back into.
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
        self.workspace
            .update(cx, |workspace, cx| workspace.set_article_mode(mode, cx));
        let editor = self
            .workspace
            .read(cx)
            .active_article()
            .and_then(|article| article.editor.clone());
        if let Some(editor) = editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
        cx.notify();
    }

    /// Whether the open document is being edited as markdown, or `None` where
    /// there is no document to be in either form.
    pub(crate) fn plain_text(&self, cx: &App) -> Option<bool> {
        let article = self.workspace.read(cx).active_article()?;
        Some(article.mode(cx) == Mode::Source)
    }

    /// Set the open page across the pane, or back in the reading column — the
    /// header menu's Full width, and `None` for its Use default width. The
    /// open one, since that is the page the menu was asked from.
    pub(crate) fn set_full_width(&mut self, wide: Option<bool>, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.set_full_width(wide, cx));
        cx.notify();
    }

    /// Cut the open article a new cover. Adding the first one comes through
    /// here too — there is nothing to choose between, only a picture to get.
    fn shuffle_cover(&mut self, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.shuffle_cover(cx));
        cx.notify();
    }

    /// Swap the generated picture for one of the person's own.
    fn pick_cover(&mut self, cx: &mut Context<Self>) {
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
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_cover(Some(&path), cx));
                cx.notify();
            });
        })
        .detach();
    }

    fn remove_cover(&mut self, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.set_cover(None, cx));
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
    /// Never off the title. That field survives a re-read that did not rebuild
    /// it, and dragging the caret out of a name somebody is typing is worse
    /// than one they have to click back into the body.
    pub(crate) fn follow_article(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.showing(cx) != Some(Pane::Article) {
            return;
        }
        let article = self.workspace.read(cx).active_article();
        let titling = article
            .and_then(|article| article.field.clone())
            .is_some_and(|field| field.focus_handle(cx).contains_focused(window, cx));
        let editor = article.and_then(|article| article.editor.clone());
        if let Some(editor) = editor.filter(|_| !titling) {
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
        let article = self.workspace.read(cx).article_in(project, at)?;
        let field = article.field.clone()?;
        let editor = article.editor.clone()?;
        let cover = article.cover.clone();
        let wide = article.wide(self.workspace.read(cx).wide_pages);
        let source_offset = source_offset(editor.read(cx), cx);
        let stale = article.stale.then(|| article.path.clone());
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
            .overflow_y_scroll()
            .track_scroll(&article.scroll)
            .flex()
            .flex_col()
            .child(self.header(cover, field, wide, cx))
            // Its own height, not the box's share of one: a long document
            // overflows and scrolls instead of being squashed and clipped,
            // and `min_h_full` is what leaves the band something to scroll
            // *under* when the document is short.
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .min_h_full()
                    .flex()
                    .justify_center()
                    .child(
                        // A page, not a paragraph. The editor's box is only
                        // as tall as the document, and a pane of dead space
                        // under a one-line note reads as something you
                        // cannot type in: the floor is what makes a click
                        // down there land a caret, and the I-beam is what
                        // says so before the click.
                        column(wide)
                            .px(px(inset(wide)))
                            .pt(px(20.))
                            .pb(px(TAIL))
                            .flex()
                            .cursor(CursorStyle::IBeam)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .ml(px(-source_offset))
                                    .child(editor),
                            ),
                    ),
            );
        Some(
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
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
                        )),
                )
                // Last, and floated over the document from where the
                // selection ends — the bar is chrome the page runs under.
                .children(self.ribbon(on, window, cx))
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
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .child(self.cover_band(cover, cx))
            .child(
                div().w_full().flex().justify_center().child(
                    column(wide)
                        .pl(px(inset(wide) + editor::Layout::of(cx).text_inset))
                        .pr(px(inset(wide)))
                        .pt(px(20.))
                        .child(field),
                ),
            )
    }

    /// What sits above the first line — the cover, or the room one would take.
    ///
    /// The band is there either way, because the alternative is a title flush
    /// against the top of the card, and it is the same height either way, so
    /// the document starts in the same place whichever it is.
    fn cover_band(&self, cover: Option<PathBuf>, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let has_cover = cover.is_some();
        div()
            .group("cover")
            .relative()
            .w_full()
            .flex_none()
            .h(px(COVER_HEIGHT))
            .overflow_hidden()
            // Empty, it has to read as somewhere a picture goes. On the card's
            // own surface it is the same tone as the page under it, so there is
            // nothing to say the band is there at all — and nothing to invite
            // the hover that would tell you.
            .when(!has_cover, |band| {
                band.bg(theme.element_hover)
                    .border_b_1()
                    .border_color(theme.border)
            })
            .children(cover.map(|path| {
                img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    // Off gpui's own asset cache, which never lets a decoded
                    // cover go. See [`crate::memory`].
                    .image_cache(&memory::covers(cx))
            }))
            .child(self.cover_controls(has_cover, cx))
    }

    /// The cover's own controls, kept off the page until the pointer is on it.
    ///
    /// Absolute, so neither showing them nor taking the cover away moves a line
    /// of the document — a row that sat in flow would shunt the first paragraph
    /// down the moment the mouse crossed the pane.
    fn cover_controls(&self, has_cover: bool, cx: &Context<Self>) -> impl IntoElement + use<> {
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
                chip(
                    "cover-shuffle",
                    if has_cover { "Shuffle" } else { "Add cover" },
                )
                .on_click(cx.listener(|this, _, _, cx| this.shuffle_cover(cx))),
            )
            .when(has_cover, |row| {
                row.child(
                    chip("cover-change", "Change")
                        .on_click(cx.listener(|this, _, _, cx| this.pick_cover(cx))),
                )
                .child(
                    chip("cover-remove", "Remove")
                        .on_click(cx.listener(|this, _, _, cx| this.remove_cover(cx))),
                )
            })
    }

    /// One article in the sidebar, under the project that holds it.
    pub(crate) fn article_row(
        &self,
        project: usize,
        ix: usize,
        title: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = !self.arranged(cx)
            && self.showing(cx) == Some(Pane::Article)
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.article == Some(ix));
        let entry = Row::Article { project, ix };
        let article = workspace
            .projects
            .get(project)
            .and_then(|open| open.articles.get(ix));
        let archived = article.is_some_and(|article| article.archived);
        let tint = sidebar::tint(selected, archived, &theme);
        let id = SharedString::from(format!("article-{project}-{ix}"));

        sidebar::row(
            id,
            "article-row",
            selected,
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
        .child(self.archive_button(("article-archive", ix), "article-row", entry, archived, cx))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_article(project, ix, window, cx);
        }))
    }
}
