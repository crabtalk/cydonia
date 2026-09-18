#[test]
fn rail_room_is_half_of_what_the_content_leaves() {
    assert_eq!(super::rail_room(400.), 0.);
    assert_eq!(super::rail_room(super::CONTENT_MAX_WIDTH), 0.);
    assert_eq!(super::rail_room(super::CONTENT_MAX_WIDTH + 60.), 30.);
}

#[test]
fn rail_requires_clear_space_in_the_conversation_pane() {
    // The question `rail` puts to bezel, for a window of this width.
    let fits = |width: f32| {
        bezel::ui::scroll::rail_fits(px(super::rail_room(width) - 2. * super::MARK_PAD))
    };
    assert!(!fits(400.));
    assert!(!fits(super::CONTENT_MAX_WIDTH));
    assert!(fits(1200.));
    // A wide window cannot supply gutter space occupied by the right panel.
    assert!(!fits(1200. - 440.));
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

/// Which marks are painted at the reading tone — one, whatever else is lit.
fn brightest(ticks: &[(gpui::Bounds<Pixels>, f32)]) -> Vec<usize> {
    ticks
        .iter()
        .enumerate()
        .filter(|(_, (_, alpha))| *alpha == MARK_READING)
        .map(|(ix, _)| ix)
        .collect()
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
fn only_the_reading_tick_is_brightest_during_hover_and_navigation(cx: &mut gpui::TestAppContext) {
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
    assert_eq!(
        initial
            .iter()
            .filter(|(_, alpha)| *alpha == MARK_READING)
            .count(),
        1
    );
    // The rail says what the pane is showing as well as where it is read from,
    // so a turn on screen beside the one being read is lit under it.
    assert!(
        initial
            .iter()
            .any(|(_, alpha)| *alpha == MARK_VISIBLE || *alpha == MARK_AWAY),
        "a session longer than the pane has marks under the reading one"
    );
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
        assert_eq!(painted[ix].1, MARK_READING);
        assert_eq!(brightest(&painted).len(), 1);
    }
    visual.simulate_mouse_move(
        gpui::point(px(90.), px(10.)),
        None,
        gpui::Modifiers::default(),
    );
    visual.run_until_parked();
    assert_eq!(brightest(&ticks(&mut visual)).len(), 1);
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
        let active = brightest(&ticks(&mut visual));
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
            painted[ix].1, MARK_READING,
            "turn {ix} should be selectable without overflow"
        );
        assert_eq!(brightest(&painted).len(), 1);
    }
}

#[test]
fn the_run_spans_every_row_painted_over_the_viewport() {
    let row = |top: f32, height: f32| {
        gpui::Bounds::new(gpui::point(px(0.), px(top)), gpui::size(px(100.), px(height)))
    };
    let viewport = row(0., 300.);
    let inset = px(60.);
    // Three rows on screen, one scrolled off the top, one under the composer.
    let painted = HashMap::from([
        (0, row(-90., 80.)),
        (1, row(-10., 80.)),
        (2, row(70., 80.)),
        (3, row(150., 80.)),
        (4, row(240., 80.)),
    ]);
    assert_eq!(painted_turns(&painted, viewport, inset), 1..4);
    assert_eq!(painted_turns(&HashMap::new(), viewport, inset), 0..0);
}

#[test]
fn a_column_taller_than_the_rail_slides_the_read_mark_into_it() {
    let step = px(MARK_THICK + 2. * MARK_PAD);
    let room = step * 10.;
    // Every mark fits, or the pane has not laid out: the column stays centred.
    assert_eq!(rail_shift(8, 7, room), px(0.));
    assert_eq!(rail_shift(20, 10, px(0.)), px(0.));
    // Taller than the opening: its ends come to rest against the opening's.
    assert_eq!(rail_shift(20, 0, room), step * 5.);
    assert_eq!(rail_shift(20, 19, room), step * -5.);
    assert_eq!(rail_shift(20, 10, room), step * -0.5);
}
