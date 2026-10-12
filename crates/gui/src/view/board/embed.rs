//! A card as a link embeds it: a view of its own that observes the card's
//! board, so whatever it is embedded in redraws when the card changes.

use super::{Docs, card_preview, resting, status_chip};
use crate::view::window::root::Cydonia;
use artifact::board::Board;
use bezel::{
    gpui::{
        AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
        Styled as _, Subscription, WeakEntity, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
};
use markdown::render::fence_band;
use std::{path::PathBuf, rc::Rc};

pub(crate) struct CardEmbed {
    board: Entity<Board>,
    card: String,
    /// The link it was embedded by, which pressing it opens.
    reference: String,
    /// Where the card's relative picture paths resolve.
    base: Option<PathBuf>,
    docs: Rc<Docs>,
    root: WeakEntity<Cydonia>,
    overflow: Entity<bool>,
    _board: Subscription,
}

impl CardEmbed {
    pub(crate) fn new(
        board: Entity<Board>,
        card: String,
        reference: String,
        base: Option<PathBuf>,
        docs: Rc<Docs>,
        root: WeakEntity<Cydonia>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            _board: cx.observe(&board, |_, _, cx| cx.notify()),
            overflow: cx.new(|_| false),
            board,
            card,
            reference,
            base,
            docs,
            root,
        }
    }
}

impl Render for CardEmbed {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let board = self.board.read(cx);
        let Some(card) = board.card(&self.card) else {
            return div().into_any_element();
        };
        let handle = board.handle_of(card).unwrap_or_default();
        let name = board.name.clone();
        let (text, status) = (card.text.clone(), card.status);
        let (doc, shortened) = self.docs.preview(&text);
        let (root, reference) = (self.root.clone(), self.reference.clone());
        div()
            .id(SharedString::from(format!("card-embed-{reference}")))
            .flex()
            .flex_col()
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(
                fence_band(&theme)
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_none()
                            .text_style(TextStyle::Caption)
                            .font_family(theme.font_mono.clone())
                            .text_color(theme.text_muted)
                            .child(handle),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_style(TextStyle::Caption)
                            .text_color(theme.text_faint)
                            .child(name),
                    )
                    .children(resting(status).map(|status| status_chip(status, &theme))),
            )
            .child(div().p(px(12.)).child(card_preview(
                &doc,
                self.base.as_deref(),
                None,
                shortened,
                self.overflow.clone(),
                window,
                cx,
            )))
            .on_click(move |_, window, cx| {
                root.update(cx, |root, cx| root.open_reference(&reference, window, cx))
                    .ok();
            })
            .into_any_element()
    }
}
