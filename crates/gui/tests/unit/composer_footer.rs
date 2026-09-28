use super::*;
use bezel::gpui::{self, Entity, Render};
use std::{cell::Cell, rc::Rc};

struct GrowingComposer(Pixels);

impl Render for GrowingComposer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_full().h(self.0)
    }
}

struct FooterView {
    composer: Entity<GrowingComposer>,
    measured: Rc<Cell<Pixels>>,
    reserved: Rc<Cell<Pixels>>,
}

impl Render for FooterView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.reserved.set(self.measured.get());
        div()
            .relative()
            .w(px(600.))
            .h(px(500.))
            .child(footer(self.composer.clone(), Some(self.measured.clone())))
    }
}

#[gpui::test]
fn composer_growth_refreshes_the_reserved_transcript_space(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let measured = Rc::new(Cell::new(px(0.)));
    let reserved = Rc::new(Cell::new(px(0.)));
    let composer = cx.new(|_| GrowingComposer(px(50.)));
    let window = cx.add_window(|_, _| FooterView {
        composer: composer.clone(),
        measured: measured.clone(),
        reserved: reserved.clone(),
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    for height in [50., 260., 80.] {
        composer.update(&mut visual, |composer, cx| {
            composer.0 = px(height);
            cx.notify();
        });
        visual.run_until_parked();
        for _ in 0..3 {
            visual.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            visual.run_until_parked();
        }
        assert_eq!(measured.get(), px(height));
        assert_eq!(reserved.get(), px(height));
    }
}
