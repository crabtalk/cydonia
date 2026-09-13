//! The article pane: one document, and the sidebar row that opens it.

use crate::{
    memory,
    model::article,
    view::{
        component::menu::Menu,
        root::{Cydonia, NewArticle, Pane},
        sidebar::{self, Row},
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, CursorStyle, Entity, Focusable as _, KeyBinding, ObjectFit,
        PathPromptOptions, SharedString, Window, actions, div, img, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{ControlSize, Sizing as _, TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
        widgets::{ButtonStyle, Buttons as _, Status as _},
    },
};
use std::path::{Path, PathBuf};

actions!(cydonia_article, [LeaveTitle]);

/// `enter` and `down` in the title move to the content. Bound on the field's
/// own context, which is the only thing deep enough to beat the field itself.
pub fn init(cx: &mut App) {
    let ctx = Some(article::TITLE_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("enter", LeaveTitle, ctx),
        KeyBinding::new("down", LeaveTitle, ctx),
    ]);
}

/// The column the document is set in, matching the transcript's.
const CONTENT_MAX_WIDTH: f32 = 720.;

/// How tall the cover band is, with a picture in it or without: half the 5:2 a
/// cover is cut at, taken at the column's width. The picture is centred in the
/// band, so what shows is the middle of it.
const COVER_HEIGHT: f32 = CONTENT_MAX_WIDTH / 5.;

/// The column's own inset. What the title adds to it is the editor's
/// [`editor::Layout::text_inset`], read at paint like the theme — the editor
/// holds its text that far inside its box so a block's drag handle has
/// somewhere to sit, and the title takes the same measure to line up with the
/// first paragraph.
const COLUMN_INSET: f32 = 24.;

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
        self.workspace
            .update(cx, |workspace, cx| workspace.open_article(project, ix, cx));
        self.pane = Pane::Article;

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
    pub(crate) fn article(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let article = self.workspace.read(cx).active_article()?;
        let field = article.field.clone()?;
        let editor = article.editor.clone()?;
        let cover = article.cover.clone();
        let stale = article.stale.then(|| article.path.clone());
        let document = div()
            .id("article")
            .on_action(cx.listener(Self::leave_title))
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&article.scroll)
            .flex()
            .flex_col()
            .child(self.header(cover, field, cx))
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
                        div()
                            .w_full()
                            .max_w(px(CONTENT_MAX_WIDTH))
                            .px(px(COLUMN_INSET))
                            .py(px(20.))
                            .flex()
                            .cursor(CursorStyle::IBeam)
                            .child(editor),
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
                .child(document)
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
                    div()
                        .w_full()
                        .max_w(px(CONTENT_MAX_WIDTH))
                        .pl(px(COLUMN_INSET + editor::Layout::of(cx).text_inset))
                        .pr(px(COLUMN_INSET))
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
        let selected = self.showing(cx) == Some(Pane::Article)
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

        sidebar::row(id, "article-row", selected, &theme)
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
            .child(
                self.menu_button(
                    ("article-menu", ix),
                    Some("article-row"),
                    icons::icon(icons::layout::Ellipsis)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                    Menu::Entry(entry),
                    cx,
                )
                .children(self.entry_menu(Menu::Entry(entry), entry, archived, cx)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_article(project, ix, window, cx);
            }))
    }
}
