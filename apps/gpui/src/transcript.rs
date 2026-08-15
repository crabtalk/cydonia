//! Virtualized transcript: one list row per `ChatItem`.
//!
//! `ListAlignment::Bottom` gives auto-follow for free — the list stays
//! pinned to the bottom until the user scrolls up, and re-pins when they
//! scroll back down. The pill jumps back when unpinned.

use crate::app::Cydonia;
use crate::markdown::{FONT_MONO, markdown};
use crate::session::{ChatItem, ChatSession, ToolStatus};
use crate::theme::Theme;
use gpui::{AnyElement, App, Entity, ListOffset, Window, div, list, prelude::*, px};

const CONTENT_MAX_WIDTH: f32 = 720.;

pub fn transcript(chat: &ChatSession, entity: Entity<Cydonia>, theme: Theme) -> AnyElement {
    let id = chat.id;
    let scrolled = chat.scrolled;
    let list_state = chat.list.clone();
    let jump_list = chat.list.clone();

    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            list(list_state, move |ix, window, cx| {
                row(&entity, id, ix, window, cx)
            })
            .size_full(),
        )
        .when(scrolled, |el| {
            el.child(
                div()
                    .id("jump-to-bottom")
                    .absolute()
                    .bottom_3()
                    .right_4()
                    .px_3()
                    .py_1()
                    .rounded_full()
                    .bg(theme.raised)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(12.))
                    .text_color(theme.text_secondary)
                    .cursor_pointer()
                    .child("↓ latest")
                    .on_click(move |_, _, _| {
                        let count = jump_list.item_count();
                        jump_list.scroll_to(ListOffset {
                            item_ix: count,
                            offset_in_item: px(0.),
                        });
                    }),
            )
        })
        .into_any_element()
}

fn row(
    entity: &Entity<Cydonia>,
    id: u64,
    ix: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = Theme::of(cx);
    let Some(item) = entity
        .read(cx)
        .session(id)
        .and_then(|chat| chat.items.get(ix))
    else {
        return div().into_any_element();
    };

    let body = match item {
        ChatItem::User(text) => div()
            .px_3()
            .py_2()
            .rounded_lg()
            .bg(theme.code_wash)
            .border_1()
            .border_color(theme.border)
            .child(markdown(text, theme.text, theme, window))
            .into_any_element(),
        ChatItem::Agent(text) => markdown(text, theme.text, theme, window),
        ChatItem::Thinking { text, done } => {
            let text = text.clone();
            let done = *done;
            div()
                .pl_3()
                .border_l_2()
                .border_color(theme.border)
                .text_color(theme.text_tertiary)
                .child(markdown(&text, theme.text_tertiary, theme, window))
                .when(!done, |el| el.opacity(0.8))
                .into_any_element()
        }
        ChatItem::Tool {
            label,
            status,
            output,
            ..
        } => {
            let dot_color = match status {
                ToolStatus::Running => theme.accent,
                ToolStatus::Success => theme.success,
                ToolStatus::Failure => theme.danger,
            };
            div()
                .rounded_md()
                .bg(theme.raised)
                .border_1()
                .border_color(theme.border)
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().size(px(8.)).rounded_full().bg(dot_color).flex_none())
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(theme.text_secondary)
                                .child(label.clone()),
                        ),
                )
                .when(!output.is_empty(), |el| {
                    el.child(
                        div()
                            .max_h(px(140.))
                            .overflow_hidden()
                            .font_family(FONT_MONO)
                            .text_size(px(12.))
                            .text_color(theme.text_tertiary)
                            .child(output.clone()),
                    )
                })
                .into_any_element()
        }
        ChatItem::Notice(text) => div()
            .text_size(px(12.))
            .text_color(theme.text_tertiary)
            .child(text.clone())
            .into_any_element(),
    };

    div()
        .px_4()
        .py_2()
        .w_full()
        .child(
            div()
                .max_w(px(CONTENT_MAX_WIDTH))
                .mx_auto()
                .w_full()
                .child(body),
        )
        .into_any_element()
}
