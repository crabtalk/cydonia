//! The article thread, drawn in the composer under an article: the transcript
//! of the article's newest session under a head row naming it.

use crate::{
    model::workspace::Workspace,
    view::{component::transcript, kit::watch::Watch, window::root::Cydonia},
};
use bezel::motion::{Fade, Painter};
use bezel::{
    gpui::{
        Context, Entity, IntoElement, Render, SharedString, WeakEntity, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, stack,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use std::path::PathBuf;

/// How much of the window's height the thread takes, and the most it takes.
const SHARE: f32 = 0.5;
const MAX_HEIGHT: f32 = 480.;

pub(crate) struct ThreadView {
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    article: PathBuf,
    /// The thread's own view of the session it shows.
    transcript: Option<Entity<transcript::SessionView>>,
    watch: Watch,
}

impl ThreadView {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        article: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            root,
            article,
            transcript: None,
            watch: Watch::default(),
        }
    }
}

impl Render for ThreadView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let id = self.workspace.read(cx).article_thread(&self.article, cx);
        let chat = id.and_then(|id| self.workspace.read(cx).session_entity(id).cloned());
        self.watch.set(&chat, cx);
        let title = id
            .and_then(|id| self.workspace.read(cx).session(id, cx))
            .map(|chat| chat.title.trim().to_owned())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "New chat".to_owned());
        let height = (f32::from(window.viewport_size().height) * SHARE).min(MAX_HEIGHT);
        let (root, article) = (self.root.clone(), self.article.clone());
        let head = stack::row()
            .flex_none()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(
                theme
                    .icon_button(
                        icons::glyph::MessageCirclePlus,
                        ButtonStyle::Ghost,
                        Some(Fade::new(Painter::of(cx), "thread-new-chat")),
                    )
                    .id("thread-new-chat")
                    .debug_selector(|| "thread-new-chat".into())
                    .flex_none()
                    .tooltip(|window, cx| Tooltip::text("New chat", window, cx))
                    .on_click(move |_, _, cx| {
                        let _ = root.update(cx, |root, cx| root.new_thread(&article, cx));
                    }),
            );
        if self.transcript.as_ref().map(|view| view.read(cx).session) != id {
            self.transcript = id.map(|id| {
                let (workspace, root) = (self.workspace.clone(), self.root.clone());
                cx.new(|cx| {
                    transcript::SessionView::new(
                        id,
                        None,
                        transcript::Drawn::Attached,
                        crate::view::window::root::COMPOSER_COLUMN,
                        workspace,
                        root,
                        cx,
                    )
                })
            });
        }
        let asked = chat.as_ref().is_some_and(|chat| {
            let chat = chat.read(cx);
            !chat.unsaid() || !chat.queue.is_empty()
        });
        let body = match self.transcript.as_ref().filter(|_| asked) {
            Some(view) => view.clone().into_any_element(),
            None => div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("Nothing asked about this article yet.")
                .into_any_element(),
        };
        stack::column()
            .id(SharedString::from("article-thread"))
            .h(px(height))
            .overflow_hidden()
            .text_color(theme.text)
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(body),
            )
    }
}
