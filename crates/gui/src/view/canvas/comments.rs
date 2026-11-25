use crate::view::canvas::CanvasView;
use artifact::canvas::comments::Anchor;
use bezel::{
    gpui::{
        AnyElement, AppContext as _, Context, Entity, Focusable as _, MouseButton, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        input::{Shape, TextField},
        scroll,
        widgets::{Button, Scaffolding as _},
    },
};

pub(super) struct Review {
    open: bool,
    selected: Option<u64>,
    draft: Option<(String, Anchor)>,
    field: Entity<TextField>,
    error: Option<String>,
}

impl Review {
    pub(super) fn new(cx: &mut Context<CanvasView>) -> Self {
        Self {
            open: false,
            selected: None,
            draft: None,
            field: cx.new(|cx| {
                TextField::new(cx)
                    .with_shape(Shape::Rows(3))
                    .with_placeholder("Write a comment")
            }),
            error: None,
        }
    }
}

impl CanvasView {
    pub(super) fn open_comments(&mut self, cx: &mut Context<Self>) {
        self.comments.open = true;
        self.comments.selected = None;
        self.comments.draft = None;
        cx.notify();
    }

    pub(super) fn show_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.open_comments(cx);
        self.comments.selected = Some(id);
    }

    pub(super) fn draft_comment(
        &mut self,
        node: String,
        anchor: Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_comments(cx);
        self.comments.draft = Some((node, anchor));
        self.comments.error = None;
        self.comments
            .field
            .update(cx, |field, cx| field.set_content("", cx));
        window.focus(&self.comments.field.focus_handle(cx), cx);
    }

    fn post_comment(&mut self, cx: &mut Context<Self>) {
        let Some((node, anchor)) = self.comments.draft.clone() else {
            return;
        };
        let note = self.comments.field.read(cx).content().trim().to_owned();
        if note.is_empty() {
            self.comments.error = Some("Write a comment before posting.".into());
            cx.notify();
            return;
        }
        if self.surface.read(cx).editor().options().read_only {
            return;
        }
        match self.source.update(cx, |source, cx| {
            source.comment_on_web(node, anchor, note, cx)
        }) {
            Ok(id) => self.show_comment(id, cx),
            Err(error) => {
                self.comments.error = Some(error);
                cx.notify();
            }
        }
    }

    pub(super) fn comment_panel(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.comments.open {
            return None;
        }
        let theme = Theme::of(cx);
        let source = self.source.read(cx);
        let mut panel =
            theme
                .group_box()
                .absolute()
                .top(px(12.))
                .right(px(12.))
                .w(px(340.))
                .max_w_full()
                .p(px(12.))
                .gap(px(8.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_style(TextStyle::Callout).child("Comments"))
                        .child(Button::new("close-canvas-comments", "Close").on_press(
                            cx.listener(|this, _, _, cx| {
                                this.comments.open = false;
                                cx.notify();
                            }),
                        )),
                );
        if let Some((_, anchor)) = &self.comments.draft {
            panel = panel
                .child(div().text_style(TextStyle::Caption).truncate().child(
                    if anchor.quote.is_empty() {
                        anchor.selector.clone()
                    } else {
                        anchor.quote.clone()
                    },
                ))
                .child(self.comments.field.clone())
                .children(
                    self.comments
                        .error
                        .as_ref()
                        .map(|error| div().text_color(theme.danger).child(error.clone())),
                )
                .child(
                    Button::new("post-canvas-comment", "Post comment")
                        .on_press(cx.listener(|this, _, _, cx| this.post_comment(cx))),
                );
        } else {
            let rows = source
                .record
                .comments
                .iter()
                .filter(|comment| {
                    self.comments
                        .selected
                        .is_none_or(|id| comment.thread.id == id)
                })
                .map(|comment| {
                    let status = if source.record.has_web_frame(&comment.node) {
                        "Web frame"
                    } else {
                        "Detached · frame removed"
                    };
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .py(px(4.))
                        .child(
                            div()
                                .text_style(TextStyle::Caption)
                                .text_color(theme.text_muted)
                                .child(format!("{status} · {}", comment.node)),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::Caption)
                                .text_color(theme.text_muted)
                                .child(comment.anchor.quote.clone()),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::Body)
                                .child(comment.thread.note.clone()),
                        )
                        .children(comment.thread.replies.iter().map(|reply| {
                            div()
                                .pl(px(8.))
                                .border_l_2()
                                .border_color(theme.text_faint)
                                .text_style(TextStyle::Body)
                                .child(reply.text.clone())
                        }))
                });
            panel = panel
                .when(self.comments.selected.is_some(), |panel| {
                    panel.child(
                        Button::new("all-canvas-comments", "All comments")
                            .on_press(cx.listener(|this, _, _, cx| this.open_comments(cx))),
                    )
                })
                .child(
                    scroll::scrolls(
                        div().id("canvas-comment-threads").max_h(px(300.)),
                        scroll::Axes::Vertical,
                    )
                    .children(rows),
                );
        }
        Some(panel.into_any_element())
    }
}
