//! The article pane: one document, and the sidebar row that opens it.

use crate::view::{
    root::{self as root, Cydonia, Pane},
    sidebar,
};
use bezel::{
    gpui::{
        AnyElement, Context, CursorStyle, Focusable as _, ObjectFit, PathPromptOptions,
        SharedString, Window, div, img, point, prelude::*, px,
    },
    theme::Theme,
    ui::icons,
};
use std::path::PathBuf;

/// The column the document is set in, matching the transcript's.
const CONTENT_MAX_WIDTH: f32 = 720.;

/// How tall the cover band is: the 5:2 a cover is cut at, taken at the column's
/// width rather than the card's, which varies. A generated cover shows whole at
/// that width and crops wider, the way Notion's does.
const COVER_HEIGHT: f32 = CONTENT_MAX_WIDTH / 2.5;

/// How much of the page sits above the document's first line, always. With a
/// cover it is what an opened article is left scrolled down to; with none it is
/// the height of the empty band standing in. Either way the title never starts
/// flush against the top of the card.
const HEADROOM: f32 = COVER_HEIGHT / 2.;

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
        // Whether this is the first look at the document, which is the only
        // time the cover is allowed to move the page: the editor is built on
        // the way in, so its absence is what says so. Coming back to an article
        // keeps where you left it.
        let settle = self
            .workspace
            .read(cx)
            .projects
            .get(project)
            .and_then(|open| open.articles.get(ix))
            .is_some_and(|article| article.editor.is_none() && article.cover.is_some());

        self.workspace
            .update(cx, |workspace, cx| workspace.open_article(project, ix, cx));
        self.pane = Pane::Article;

        let (editor, scroll) = {
            let article = self.workspace.read(cx).active_article();
            (
                article.and_then(|article| article.editor.clone()),
                article.map(|article| article.scroll.clone()),
            )
        };
        if let Some(scroll) = scroll.filter(|_| settle) {
            scroll.set_offset(point(px(0.), px(-HEADROOM)));
        }
        if let Some(editor) = editor {
            window.focus(&editor.focus_handle(cx), cx);
        }
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
        let editor = article.editor.clone()?;
        let cover = article.cover.clone();
        Some(
            div()
                .id("article")
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scroll()
                .track_scroll(&article.scroll)
                .flex()
                .flex_col()
                .child(self.cover_band(cover, cx))
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
                                .px(px(24.))
                                .py(px(20.))
                                .flex()
                                .cursor(CursorStyle::IBeam)
                                .child(editor),
                        ),
                )
                .into_any_element(),
        )
    }

    /// What sits above the first line — the cover, or the room one would take.
    ///
    /// The band is there either way, because the alternative is a title flush
    /// against the top of the card. Without a picture it is [`HEADROOM`] of
    /// nothing, which is also what is left of a cover once the page has
    /// settled: the document starts in the same place whichever it is.
    fn cover_band(&self, cover: Option<PathBuf>, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let has_cover = cover.is_some();
        div()
            .group("cover")
            .relative()
            .w_full()
            .flex_none()
            .h(px(if has_cover { COVER_HEIGHT } else { HEADROOM }))
            // Runs the card's full width, and the card's own rounding is what
            // cuts its top corners.
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
        let chip = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px(px(8.))
                .py(px(3.))
                .rounded(px(Theme::control_radius()))
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .hover(|chip| chip.bg(theme.element_hover).text_color(theme.text))
                .child(label)
        };

        div()
            .absolute()
            .bottom(px(10.))
            .right(px(10.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(Theme::control_radius()))
            // Its own plate, because what it sits on is a picture we did not
            // choose: on the cover's own colours there is no text tone that
            // stays legible without one.
            .bg(theme.surface_raised)
            .border_1()
            .border_color(theme.border)
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
            .py(px(6.))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
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
                    .text_size(px(root::SIDEBAR_TEXT))
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
