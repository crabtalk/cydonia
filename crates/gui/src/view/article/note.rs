//! The text of an open comment: the words it is on, which close it on a press,
//! and its note and replies, each one selectable and copied the way the
//! transcript's prose is.

use super::comment_paint;
use crate::{
    model::workspace::Workspace,
    view::kit::{bubble, selection::Selections},
};
use artifact::article::comments::{Author, Comment};
use bezel::{
    gpui::{
        AnyElement, Context, ElementId, Entity, IntoElement, Render, SharedString, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::scroll as scrollbars,
};
use markdown::selectable;
use std::{path::PathBuf, rc::Rc};

/// The most an open note's thread grows before it scrolls.
const NOTE_MAX_HEIGHT: f32 = 320.;

/// Which of an open comment's texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Part {
    Note,
    Reply(usize),
}

pub(crate) struct NoteText {
    workspace: Entity<Workspace>,
    /// The article the comment is on.
    path: PathBuf,
    id: u64,
    selection: Selections<Part>,
}

impl NoteText {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        path: PathBuf,
        id: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            path,
            id,
            selection: Selections::default(),
        }
    }

    fn comment(&self, cx: &Context<Self>) -> Option<Comment> {
        let (_, article) = self.workspace.read(cx).article_at(&self.path)?;
        article
            .comments
            .comments
            .iter()
            .find(|comment| comment.id == self.id)
            .cloned()
    }

    /// `text` as selectable prose, under `part`.
    fn prose(
        &mut self,
        part: Part,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let doc = Rc::new(markdown::parse(text));
        let layouts = self.selection.layouts(part);
        let body = selectable::render_with(
            ElementId::Name(SharedString::from(format!("comment-{}-{part:?}", self.id))),
            &doc,
            &layouts,
            self.selection.of(part),
            self.selection.dragging_in(part),
            markdown::Editing::default(),
            window,
            cx,
            {
                let doc = doc.clone();
                move |this: &mut Self, pointer, cx| {
                    this.selection.point(part, pointer, &doc);
                    cx.notify();
                }
            },
        );
        let focus = self.selection.focus(part, cx);
        selectable::surface(&focus, &doc, self.selection.of(part), cx)
            .child(body)
            .into_any_element()
    }
}

impl Render for NoteText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.selection
            .clear_on_blur(window, cx, |note: &mut Self| &mut note.selection);
        let theme = Theme::of(cx).clone();
        let Some(comment) = self.comment(cx) else {
            return div().into_any_element();
        };
        let quote = div()
            .id(("comment-quote", self.id))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                let path = this.path.clone();
                this.workspace.update(cx, |workspace, cx| {
                    workspace.activate_comment(&path, None, cx)
                });
            }))
            .flex()
            .items_start()
            .gap(px(6.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl(px(8.))
                    .border_l_2()
                    .border_color(comment_paint(&theme).color)
                    .max_h(px(TextStyle::Callout.painted_line_height() * 2.))
                    .overflow_hidden()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(artifact::text::one_line(&comment.quote)),
            )
            .when(comment.detached(), |el| {
                el.child(
                    div()
                        .flex_none()
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_faint)
                        .child("Outdated"),
                )
            });
        let note = self.prose(Part::Note, &comment.note, window, cx);
        let replies: Vec<AnyElement> = comment
            .replies
            .iter()
            .enumerate()
            .map(|(ix, reply)| {
                let text = self.prose(Part::Reply(ix), &reply.text, window, cx);
                match reply.from {
                    Author::Reader => bubble::bubble(&theme).child(text).into_any_element(),
                    Author::Agent => text,
                }
            })
            .collect();
        let waiting = comment
            .replies
            .last()
            .is_some_and(|reply| reply.from == Author::Reader);
        let thread = div()
            .id(("comment-note", self.id))
            .max_h(px(NOTE_MAX_HEIGHT))
            .map(|el| scrollbars::scrolls(el, scrollbars::Axes::Vertical))
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_style(TextStyle::Body)
            .child(note)
            .children(replies)
            .when(waiting, |el| {
                el.child(
                    div()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_faint)
                        .child("Waiting for an answer…"),
                )
            });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(quote)
            .child(thread)
            .into_any_element()
    }
}
