use super::*;

fn startup() -> ChatItem {
    ChatItem::Process {
        command: "agent".into(),
        output: "startup warning".into(),
    }
}

#[test]
fn startup_logs_do_not_create_a_turn() {
    assert!(turns(&[]).is_empty());
    assert!(turns(&[startup()]).is_empty());
    let turns = turns(&[
        startup(),
        ChatItem::User("hello".into()),
        ChatItem::Agent("hi".into()),
    ]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].range, 1..3);
}

#[test]
fn a_failure_before_the_first_prompt_is_visible() {
    let turns = turns(&[ChatItem::Notice {
        text: "connection failed".into(),
        failed: true,
    }]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].range, 0..1);
}

#[test]
fn startup_tool_failures_are_included_in_the_work() {
    let turns = turns(&[ChatItem::Tool {
        id: "startup".into(),
        kind: ToolKind::Other,
        label: "MCP startup".into(),
        status: ToolStatus::Failure,
        output: "connection failed".into(),
    }]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].range, 0..1);
}

#[test]
fn prose_before_a_trailing_tool_call_is_not_work() {
    let items = [
        ChatItem::User("why".into()),
        ChatItem::Agent("the long answer".into()),
        ChatItem::Tool {
            id: "status".into(),
            kind: ToolKind::Other,
            label: "board_set_card_status".into(),
            status: ToolStatus::Success,
            output: String::new(),
        },
        ChatItem::Agent("tagged".into()),
    ];
    assert!(!interim(&items[1]));
    assert!(interim(&items[2]));
    assert!(!interim(&items[3]));
}

#[gpui::test]
fn selecting_message_text_takes_focus_from_the_composer(cx: &mut gpui::TestAppContext) {
    use crate::view::component::composer::Composer;
    use bezel::gpui::{self, Focusable as _, Render};
    struct Messages {
        composer: gpui::Entity<Composer>,
        user: gpui::FocusHandle,
        agent: gpui::FocusHandle,
    }
    impl Render for Messages {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    selectable::surface(&self.user, &markdown::parse("User message"), None, cx)
                        .id("user-text")
                        .debug_selector(|| "user-text".into())
                        .h(px(40.))
                        .child("User message"),
                )
                .child(
                    selectable::surface(&self.agent, &markdown::parse("Agent response"), None, cx)
                        .id("agent-text")
                        .debug_selector(|| "agent-text".into())
                        .h(px(40.))
                        .child("Agent response"),
                )
                .child(self.composer.clone())
        }
    }
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let composer = cx.new(Composer::new);
    let window = cx.add_window(|window, cx| {
        window.focus(&composer.focus_handle(cx), cx);
        Messages {
            composer: composer.clone(),
            user: cx.focus_handle(),
            agent: cx.focus_handle(),
        }
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for selector in ["user-text", "agent-text"] {
        let point = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_click(point, gpui::Modifiers::default());
        visual.run_until_parked();
        window
            .update(&mut visual, |messages, window, cx| {
                let focus = if selector == "user-text" {
                    &messages.user
                } else {
                    &messages.agent
                };
                assert!(focus.is_focused(window));
                assert!(!messages.composer.focus_handle(cx).is_focused(window));
            })
            .unwrap();
    }
}

#[test]
fn only_the_item_the_press_landed_in_is_told_it_is_dragging() {
    use markdown::{Cursor, Part};
    let mut state = State::default();
    assert!(!state.dragging_in(0));
    state.point(1, Pointer::Down(Cursor::new(0, Part::Body, 0)));
    assert!(state.dragging_in(1));
    assert!(!state.dragging_in(0), "an item holding no selection is idle");
    state.point(1, Pointer::Up);
    assert!(!state.dragging_in(1));
}
