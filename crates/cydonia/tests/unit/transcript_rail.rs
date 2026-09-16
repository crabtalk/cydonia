use super::active_turn;
use gpui::{
    Context, Render, ScrollHandle, TestAppContext, VisualTestContext, Window, div, point,
    prelude::*, px,
};

struct Transcript {
    scroll: ScrollHandle,
    heights: Vec<f32>,
}

impl Render for Transcript {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("transcript")
            .w(px(400.))
            .h(px(300.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .pb(px(100.))
            .children(self.heights.iter().map(|height| div().h(px(*height))))
    }
}

#[gpui::test]
fn bottom_selects_short_latest_turn_and_scrolling_back_restores_top(cx: &mut TestAppContext) {
    let scroll = ScrollHandle::new();
    let window = cx.add_window(|_, _| Transcript {
        scroll: scroll.clone(),
        heights: vec![400., 100., 40.],
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    assert!(scroll.max_offset().y > px(0.));
    assert_eq!(active_turn(&scroll, 3), 0);

    scroll.set_offset(point(px(0.), -scroll.max_offset().y));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(scroll.top_item() < 2, "latest turn cannot reach the top");
    assert_eq!(active_turn(&scroll, 3), 2);

    scroll.set_offset(point(px(0.), -scroll.max_offset().y + px(20.)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(active_turn(&scroll, 3), scroll.top_item());
}

#[gpui::test]
fn transcript_that_fits_selects_latest_turn(cx: &mut TestAppContext) {
    let scroll = ScrollHandle::new();
    let window = cx.add_window(|_, _| Transcript {
        scroll: scroll.clone(),
        heights: vec![40., 40.],
    });
    let cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    assert_eq!(scroll.max_offset().y, px(0.));
    assert_eq!(active_turn(&scroll, 2), 1);
    assert_eq!(active_turn(&scroll, 1), 0);
    assert_eq!(active_turn(&scroll, 0), 0);
}

#[test]
fn rail_requires_clear_space_in_the_conversation_pane() {
    let required = bezel::ui::scroll::RAIL_ROOM + 2. * super::MARK_PAD;
    assert_eq!(super::rail_room(400.), 0.);
    assert_eq!(super::rail_room(super::CONTENT_MAX_WIDTH), 0.);
    assert!(super::rail_room(super::CONTENT_MAX_WIDTH + 2. * required - 1.) < required);
    assert_eq!(
        super::rail_room(super::CONTENT_MAX_WIDTH + 2. * required),
        required
    );
    // A wide window cannot supply gutter space occupied by the right panel.
    assert!(super::rail_room(1200.) >= required);
    assert!(super::rail_room(1200. - 440.) < required);
}
