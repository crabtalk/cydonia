use super::*;
use bezel::gpui::{TestAppContext, VisualTestContext, size};
use bezel::theme::Appearance;

#[test]
fn revealing_moves_only_the_hidden_edge() {
    assert_eq!(reveal_delta(px(40.), px(100.), px(20.), px(200.)), px(0.));
    assert_eq!(
        reveal_delta(px(170.), px(230.), px(20.), px(200.)),
        px(-30.)
    );
    assert_eq!(reveal_delta(px(10.), px(70.), px(20.), px(200.)), px(10.));
}

#[test]
fn a_card_taller_than_the_exposed_lane_aligns_at_the_top() {
    assert_eq!(reveal_delta(px(60.), px(240.), px(20.), px(100.)), px(-40.));
    assert_eq!(reveal_delta(px(20.), px(200.), px(20.), px(100.)), px(0.));
}

#[test]
fn closing_restores_automatic_scrolling_but_preserves_manual_scrolling() {
    let scroll = ScrollHandle::new();
    let before = gpui::point(px(0.), px(-30.));
    let after = gpui::point(px(0.), px(-120.));
    let adjustment = LaneAdjustment {
        scroll: scroll.clone(),
        before,
        after,
    };
    scroll.set_offset(after);
    adjustment.restore();
    assert_eq!(scroll.offset(), before);

    let manual = gpui::point(px(0.), px(-160.));
    scroll.set_offset(manual);
    adjustment.restore();
    assert_eq!(scroll.offset(), manual);
}

struct Preview {
    doc: markdown::Doc,
    overflow: Entity<bool>,
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(240.))
            .child(card_preview(&self.doc, self.overflow.clone(), window, cx))
    }
}

#[gpui::test]
fn rendered_overflow_updates_when_a_card_is_shortened(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Preview {
        doc: markdown::parse(&"a paragraph\n\n".repeat(30)),
        overflow: cx.new(|_| false),
    });
    let page = window.root(cx).unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(300.), px(400.)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| *page.read(cx).overflow.read(cx)));

    cx.update(|_, cx| {
        page.update(cx, |page, cx| {
            page.doc = markdown::parse("Short card");
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| *page.read(cx).overflow.read(cx)));
}
