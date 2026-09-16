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

use super::*;
use bezel::gpui::{self, Render};

struct RailView(ChatSession, f32);

impl Render for RailView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w(px(100.))
            .h(px(self.1))
            .child(self.0.transcript.list.render(
                |ix, _, _| {
                    div()
                        .h(if ix == 7 {
                            px(root::composer_height() + root::COMPOSER_BOTTOM + PAD)
                        } else {
                            px(80.)
                        })
                        .into_any_element()
                },
                |_, _, _| {},
            ))
            .child(rail(&self.0, &turns(&self.0.items), px(100.)))
    }
}

fn ticks(cx: &mut gpui::VisualTestContext) -> Vec<(gpui::Bounds<Pixels>, f32)> {
    cx.update(|window, _| {
        window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                quad.bounds
                    .size
                    .map(|value| px(value.0 / window.scale_factor()))
                    == gpui::size(px(MARK), px(MARK_THICK))
            })
            .map(|quad| {
                (
                    quad.bounds.map(|value| px(value.0 / window.scale_factor())),
                    quad.background.as_solid().unwrap().a,
                )
            })
            .collect()
    })
}

#[gpui::test]
fn only_active_tick_is_highlighted_during_hover_and_navigation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window = cx.add_window(|_, _| {
        let record = serde_json::from_value(serde_json::json!({
            "id":"rail", "agent":"test", "title":"", "name":null, "updated":1, "items":[]
        }))
        .unwrap();
        let mut chat = ChatSession::restore(
            1,
            "/nonexistent/rail-test".into(),
            crate::model::settings::Agent {
                name: "test".into(),
                id: None,
                command: String::new(),
                args: vec![],
                env: Default::default(),
            },
            record,
        );
        for ix in 0..7 {
            chat.items.push(ChatItem::User(format!("Question {ix}")));
            chat.items.push(ChatItem::Agent(format!("Answer {ix}")));
        }
        chat.transcript.list.sync((0..8).collect());
        RailView(chat, 300.)
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(&mut visual, |view, _, _| {
            assert_eq!(
                view.0.transcript.list.state.viewport_bounds().size.height,
                px(300.)
            );
        })
        .unwrap();
    let initial = ticks(&mut visual);
    assert_eq!(initial.len(), 7);
    assert_eq!(initial.iter().filter(|(_, alpha)| *alpha == 0.6).count(), 1);
    for ix in [0, 1, 2, 6, 5, 4] {
        visual.simulate_mouse_move(initial[ix].0.center(), None, gpui::Modifiers::default());
        visual.run_until_parked();
        assert_eq!(
            ticks(&mut visual),
            initial,
            "hover must not change the active highlight"
        );
    }
    for ix in [0, 3, 4, 5, 6] {
        visual.simulate_click(initial[ix].0.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        visual.update(|window, _| window.refresh());
        visual.run_until_parked();
        let painted = ticks(&mut visual);
        assert_eq!(painted[ix].1, 0.6);
        assert_eq!(painted.iter().filter(|(_, alpha)| *alpha > 0.2).count(), 1);
    }
    visual.simulate_mouse_move(
        gpui::point(px(90.), px(10.)),
        None,
        gpui::Modifiers::default(),
    );
    visual.run_until_parked();
    assert_eq!(
        ticks(&mut visual)
            .iter()
            .filter(|(_, alpha)| *alpha > 0.2)
            .count(),
        1
    );
    let max = window
        .update(&mut visual, |view, _, _| {
            view.0.transcript.list.state.max_offset_for_scrollbar().y
        })
        .unwrap();
    assert!(max > px(0.));
    let mut reached = std::collections::HashSet::new();
    for step in 0..=100 {
        window
            .update(&mut visual, |view, window, _| {
                view.0
                    .transcript
                    .list
                    .state
                    .set_offset_from_scrollbar(gpui::point(px(0.), -max * (step as f32 / 100.)));
                window.refresh();
            })
            .unwrap();
        visual.run_until_parked();
        let painted = ticks(&mut visual);
        let active: Vec<_> = painted
            .iter()
            .enumerate()
            .filter(|(_, (_, alpha))| *alpha > 0.2)
            .map(|(ix, _)| ix)
            .collect();
        assert_eq!(active.len(), 1);
        reached.insert(active[0]);
    }
    assert_eq!(reached.len(), 7, "scrolling should reach every short turn");

    window
        .update(&mut visual, |view, _, cx| {
            view.1 = 1000.;
            cx.notify();
        })
        .unwrap();
    visual.simulate_resize(gpui::size(px(100.), px(1000.)));
    visual.run_until_parked();
    let positions = ticks(&mut visual);
    for ix in 0..7 {
        visual.simulate_click(positions[ix].0.center(), gpui::Modifiers::default());
        visual.run_until_parked();
        visual.update(|window, _| window.refresh());
        visual.run_until_parked();
        let painted = ticks(&mut visual);
        assert_eq!(
            painted[ix].1, 0.6,
            "turn {ix} should be selectable without overflow"
        );
        assert_eq!(painted.iter().filter(|(_, alpha)| *alpha > 0.2).count(), 1);
    }
}
