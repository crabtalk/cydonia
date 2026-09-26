use bezel::{
    gpui::{self, Context, Render, ScrollHandle, Window, div, point, prelude::*, px},
    theme::{Appearance, Theme},
    ui::{
        list::VariableList,
        scroll::{self, FollowState},
    },
};

struct Legacy {
    handle: ScrollHandle,
    follow: FollowState,
    height: f32,
}

impl Render for Legacy {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w(px(200.))
            .h(px(200.))
            .child(
                div()
                    .id("legacy-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.handle)
                    .child(div().h(px(self.height))),
            )
            .child(scroll::follow(&self.handle, &self.follow))
    }
}

#[gpui::test]
fn legacy_follow_reproduces_tail_stickiness(cx: &mut gpui::TestAppContext) {
    let window = cx.add_window(|_, _| Legacy {
        handle: ScrollHandle::new(),
        follow: FollowState::new(),
        height: 1000.,
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for (up, growth) in [(3., 0.), (20., 20.)] {
        window
            .update(&mut visual, |view, window, _| {
                let offset = view.handle.offset();
                assert_eq!(offset.y, -view.handle.max_offset().y);
                view.handle.set_offset(point(px(0.), offset.y + px(up)));
                view.height += growth;
                window.refresh();
            })
            .unwrap();
        visual.run_until_parked();
        window
            .update(&mut visual, |view, _, _| {
                assert!(view.follow.following());
                assert_eq!(
                    view.handle.offset().y,
                    -view.handle.max_offset().y,
                    "upward movement was snapped back"
                );
            })
            .unwrap();
    }
}

struct Current {
    list: VariableList<usize>,
    height: f32,
}

impl Render for Current {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let height = self.height;
        div().w(px(200.)).h(px(200.)).child(super::viewport(
            &self.list.state,
            self.list.render(
                move |_, _, _| div().h(px(height)).into_any_element(),
                |_, _, _| {},
            ),
        ))
    }
}

#[gpui::test]
fn virtual_list_releases_tail_on_first_wheel_movement(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, _| {
        let list = VariableList::default();
        list.sync(vec![0]);
        Current {
            list,
            height: 1000.,
        }
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for (delta, growth) in [
        (gpui::ScrollDelta::Pixels(point(px(0.), px(0.25))), 0.),
        (gpui::ScrollDelta::Pixels(point(px(0.), px(3.))), 0.),
        (gpui::ScrollDelta::Lines(point(0., 1.)), 20.),
    ] {
        window
            .update(&mut visual, |view, window, _| {
                view.list.state.set_follow_mode(gpui::FollowMode::Tail);
                window.refresh();
            })
            .unwrap();
        visual.run_until_parked();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(100.), px(100.)),
            delta,
            ..Default::default()
        });
        window
            .update(&mut visual, |view, window, _| {
                view.height += growth;
                view.list.invalidate(&0);
                window.refresh();
            })
            .unwrap();
        visual.run_until_parked();
        window
            .update(&mut visual, |view, _, _| {
                assert!(!view.list.state.is_following_tail());
                let distance = view.list.state.max_offset_for_scrollbar().y
                    + view.list.state.scroll_px_offset_for_scrollbar().y;
                assert!(
                    distance > px(0.),
                    "first wheel movement should leave the bottom"
                );
            })
            .unwrap();

        let before = window
            .update(&mut visual, |view, window, _| {
                let before = view.list.state.scroll_px_offset_for_scrollbar().y;
                view.height += 40.;
                view.list.invalidate(&0);
                window.refresh();
                before
            })
            .unwrap();
        visual.run_until_parked();
        window
            .update(&mut visual, |view, _, _| {
                assert_eq!(view.list.state.scroll_px_offset_for_scrollbar().y, before);
                assert!(!view.list.state.is_following_tail());
            })
            .unwrap();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(100.), px(100.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            ..Default::default()
        });
        visual.run_until_parked();
        window
            .update(&mut visual, |view, _, _| {
                assert!(
                    view.list.state.is_following_tail(),
                    "downward scrolling to the end resumes following"
                );
            })
            .unwrap();
    }
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(100.), px(100.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(0.25))),
        ..Default::default()
    });
    visual.run_until_parked();
    window
        .update(&mut visual, |view, window, _| {
            assert!(!view.list.state.is_following_tail());
            view.list.state.scrollbar_drag_started();
            view.list.state.set_offset_from_scrollbar(point(
                px(0.),
                -view.list.state.max_offset_for_scrollbar().y,
            ));
            window.refresh();
        })
        .unwrap();
    visual.run_until_parked();
    window
        .update(&mut visual, |view, _, _| {
            assert!(
                view.list.state.is_following_tail(),
                "dragging to the end resumes following"
            );
            view.list.state.scrollbar_drag_ended();
        })
        .unwrap();
}
