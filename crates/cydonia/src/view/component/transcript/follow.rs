use bezel::gpui::{Div, FollowMode, IntoElement, ListState, canvas, prelude::*, px};

/// Upward input releases following even inside GPUI's one-pixel bottom tolerance.
pub(super) fn viewport(state: &ListState, content: Div) -> impl IntoElement {
    let wheel = state.clone();
    let drag = state.clone();
    content
        .id("transcript-scroll")
        .on_scroll_wheel(move |event, _, _| {
            let delta = event.delta.pixel_delta(px(20.)).y;
            if delta > px(0.) && wheel.max_offset_for_scrollbar().y > px(0.) {
                wheel.set_follow_mode(FollowMode::Normal);
            } else if delta < px(0.) && at_end(&wheel) && !wheel.is_following_tail() {
                wheel.set_follow_mode(FollowMode::Tail);
            }
        })
        .child(
            canvas(
                move |_, window, _| {
                    if drag.is_scrollbar_dragging() && at_end(&drag) && !drag.is_following_tail() {
                        drag.set_follow_mode(FollowMode::Tail);
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
}

fn at_end(state: &ListState) -> bool {
    state.max_offset_for_scrollbar().y + state.scroll_px_offset_for_scrollbar().y <= px(0.)
}

#[cfg(test)]
#[path = "../../../../tests/unit/transcript_follow.rs"]
mod tests;
