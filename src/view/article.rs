//! The article pane: one document, and the sidebar row that opens it.

use crate::{
    model::article,
    view::{
        root::{Cydonia, Pane},
        sidebar,
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
        widgets::{ButtonStyle, Buttons as _},
    },
};
use std::path::PathBuf;

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

    fn delete_article(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.delete_article(project, ix, cx);
        });
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
        Some(
            div()
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
                )
                .into_any_element(),
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
            .children(cover.map(|path| img(path).size_full().object_fit(ObjectFit::Cover)))
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
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = self.showing(cx) == Pane::Article
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.article == Some(ix));
        let id = SharedString::from(format!("article-{project}-{ix}"));

        sidebar::row(id, "article-row", selected, &theme)
            .child(
                icons::icon(icons::DOCUMENT)
                    .size(px(14.))
                    .flex_none()
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_muted
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Body)
                    .text_color(if selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .child(title),
            )
            .child(
                div()
                    .id(("delete-article", ix))
                    .flex_none()
                    .invisible()
                    .group_hover("article-row", |el| el.visible())
                    .rounded(px(Theme::control_radius()))
                    .p(px(2.))
                    .child(
                        icons::icon(icons::TRASH_BIN_MINIMALISTIC)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.delete_article(project, ix, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_article(project, ix, window, cx);
            }))
    }
}
