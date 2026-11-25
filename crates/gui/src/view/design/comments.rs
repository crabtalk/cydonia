use crate::view::design::DesignView;
use artifact::{comment::Anchor, design::Page};
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
    pub(super) fn close(&mut self) {
        self.open = false;
        self.selected = None;
        self.draft = None;
        self.error = None;
    }

    pub(super) fn new(cx: &mut Context<DesignView>) -> Self {
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

impl DesignView {
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
        let keep = node.clone();
        self.write(
            |design| {
                if let Some(page) = design.pages.iter_mut().find(|page| page.id == node) {
                    page.comment(anchor, note);
                }
            },
            cx,
        );
        if self.error.is_none()
            && let Some(id) = self
                .source
                .read(cx)
                .pages
                .iter()
                .find(|page| page.id == keep)
                .and_then(|page| page.comments.last())
                .map(|comment| comment.thread.id)
        {
            self.show_comment(id, cx);
        }
    }

    pub(super) fn comment_panel(&self, page: &Page, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.comments.open {
            return None;
        }
        let theme = Theme::of(cx);
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
                        .child(Button::new("close-design-comments", "Close").on_press(
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
                    Button::new("post-design-comment", "Post comment")
                        .on_press(cx.listener(|this, _, _, cx| this.post_comment(cx))),
                );
        } else {
            let rows = page
                .comments
                .iter()
                .filter(|comment| {
                    self.comments
                        .selected
                        .is_none_or(|id| comment.thread.id == id)
                })
                .map(|comment| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .py(px(4.))
                        .when(
                            page.revision.is_none() || self.detached(comment.thread.id),
                            |row| {
                                row.child(
                                    div()
                                        .text_style(TextStyle::Caption)
                                        .text_color(theme.text_muted)
                                        .child("Detached"),
                                )
                            },
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
                        Button::new("all-design-comments", "All comments")
                            .on_press(cx.listener(|this, _, _, cx| this.open_comments(cx))),
                    )
                })
                .child(
                    scroll::scrolls(
                        div().id("design-comment-threads").max_h(px(300.)),
                        scroll::Axes::Vertical,
                    )
                    .children(rows),
                );
        }
        Some(panel.into_any_element())
    }
}
