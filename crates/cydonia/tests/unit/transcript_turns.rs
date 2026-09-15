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
    assert_eq!(turns[0].answer_from, 2);
}

#[test]
fn a_failure_before_the_first_prompt_is_visible() {
    let turns = turns(&[ChatItem::Notice {
        text: "connection failed".into(),
        failed: true,
    }]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].answer_from, 0);
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
    assert_eq!(turns[0].answer_from, 1);
}
