//! The virtual list's thumb: when it shows, when it fades, and what holds it up.

use bezel::{
    gpui::{self, Context, Render, Window, div, point, prelude::*, px},
    theme::{Appearance, Theme},
    ui::{
        list::VariableList,
        scroll::{self, Visibility},
    },
};

struct Feed(VariableList<usize>);
impl Render for Feed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(300.)).child(
            self.0
                .render(|_, _, _| div().h(px(40.)).into_any_element(), |_, _, _| {}),
        )
    }
}

fn thumb(cx: &mut gpui::VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.update(|window, _| {
        window
            .painted_quads()
            .into_iter()
            .map(|quad| quad.bounds.map(|value| px(value.0 / window.scale_factor())))
            .find(|bounds| bounds.size.width == px(4.))
    })
}

/// The bar draws from the previous frame's geometry, and an animation only
/// advances over frames that are drawn.
fn frame(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn idle(cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(scroll::TRANSIENT_IDLE);
    frame(cx);
}

#[gpui::test]
fn the_thumb_fades_out_and_hover_or_a_drag_holds_it_up(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        scroll::set_visibility(Visibility::Scrolling, cx);
    });
    let list = VariableList::default();
    list.sync((0..30).collect());
    let window = cx.add_window(|_, _| Feed(list.clone()));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    frame(&mut visual);
    assert!(
        thumb(&mut visual).is_some(),
        "the content arriving shows it"
    );
    idle(&mut visual);
    assert!(thumb(&mut visual).is_none(), "and an idle window hides it");

    // A wheel is activity.
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(200.), px(100.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(30.))),
        ..Default::default()
    });
    frame(&mut visual);
    let bounds = thumb(&mut visual).expect("scrolling shows the thumb");
    assert_eq!(bounds.right(), px(397.));
    idle(&mut visual);
    assert!(thumb(&mut visual).is_none());

    // The strip is a hitbox now, and a wheel over it still belongs to the list.
    let before = list.state.scroll_px_offset_for_scrollbar().y;
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(396.), px(100.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(30.))),
        ..Default::default()
    });
    frame(&mut visual);
    assert_ne!(
        list.state.scroll_px_offset_for_scrollbar().y,
        before,
        "the track swallowed a wheel meant for the rows"
    );

    // The pointer on the strip raises it and holds it there, which is what
    // makes a settled bar grabbable at all.
    visual.simulate_mouse_move(point(px(395.), px(100.)), None, gpui::Modifiers::default());
    frame(&mut visual);
    let bounds = thumb(&mut visual).expect("hovering the strip shows the thumb");
    idle(&mut visual);
    assert!(thumb(&mut visual).is_some(), "hover holds the thumb up");

    // A drag holds it up too, wherever the pointer ends up.
    visual.simulate_mouse_down(
        bounds.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    visual.run_until_parked();
    assert!(list.state.is_scrollbar_dragging());
    visual.simulate_mouse_move(
        point(px(200.), px(260.)),
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    frame(&mut visual);
    idle(&mut visual);
    assert!(thumb(&mut visual).is_some(), "a drag holds the thumb up");
    visual.simulate_mouse_up(
        point(px(200.), px(260.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    frame(&mut visual);
    assert!(!list.state.is_scrollbar_dragging());
    idle(&mut visual);
    assert!(
        thumb(&mut visual).is_none(),
        "released away from the strip, it fades"
    );

    visual.update(|_, cx| scroll::set_visibility(Visibility::Always, cx));
    frame(&mut visual);
    idle(&mut visual);
    assert!(
        thumb(&mut visual).is_some(),
        "always outlasts the idle window"
    );
    visual.update(|_, cx| scroll::set_visibility(Visibility::Never, cx));
    frame(&mut visual);
    assert!(thumb(&mut visual).is_none());
}
