//! The article pane: one document, and the rail row that opens it.

use crate::view::root::{self as root, Cydonia, Pane};
use bezel::{
    gpui::{
        AnyElement, Context, CursorStyle, Focusable as _, SharedString, Window, div, prelude::*, px,
    },
    theme::Theme,
    ui::icons,
};

/// The column the document is set in, matching the transcript's.
const CONTENT_MAX_WIDTH: f32 = 720.;

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
        Some(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .justify_center()
                .child(
                    div().w_full().max_w(px(CONTENT_MAX_WIDTH)).child(
                        div()
                            .id("article")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&article.scroll)
                            // Start, not stretch: the page takes its own
                            // height so a long document overflows the box
                            // and scrolls, instead of being squashed into
                            // it and clipped.
                            .items_start()
                            .child(
                                // A page, not a paragraph. The editor's box
                                // is only as tall as the document, and a
                                // pane of dead space under a one-line note
                                // reads as something you cannot type in:
                                // the floor is what makes a click down
                                // there land a caret, and the I-beam is
                                // what says so before the click.
                                div()
                                    .w_full()
                                    .min_h_full()
                                    .px(px(24.))
                                    .py(px(20.))
                                    .flex()
                                    .cursor(CursorStyle::IBeam)
                                    .child(editor),
                            ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// One article in the rail, under the project that holds it.
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

        root::rail_row(id, "article-row", selected, &theme)
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
                    .text_size(px(root::RAIL_TEXT))
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
