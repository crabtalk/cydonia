use super::*;
use artifact::project::fs;
use bezel::gpui;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "cydonia-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn chat(&self) -> ChatSession {
        let record = serde_json::from_value(serde_json::json!({
            "id": "test", "agent": "test", "title": "", "name": null,
            "updated": 1, "items": []
        }))
        .unwrap();
        ChatSession::restore(
            1,
            artifact::location::Location::from(self.0.clone()),
            agent("/bin/false", &[]),
            record,
        )
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn agent(command: &str, args: &[&str]) -> settings::Agent {
    settings::Agent {
        name: "test".into(),
        id: None,
        command: command.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        env: Default::default(),
    }
}

fn update(value: serde_json::Value) -> Event {
    Event::Update(serde_json::from_value(value).unwrap())
}

#[gpui::test]
fn background_events_do_not_move_an_older_session_above_a_new_one(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new();
    let older = cx.new(|_| scratch.chat());
    let mut newer = scratch.chat();
    newer.updated = SystemTime::now();
    let before = older.read_with(cx, |older, _| older.touched());
    for event in [
        Event::Stderr("background diagnostic".into()),
        update(serde_json::json!({"sessionUpdate": "session_info_update", "title": "A title"})),
        update(
            serde_json::json!({"sessionUpdate": "available_commands_update", "availableCommands": []}),
        ),
        update(serde_json::json!({"sessionUpdate": "usage_update", "used": 1, "size": 100})),
        update(
            serde_json::json!({"sessionUpdate": "current_mode_update", "currentModeId": "code"}),
        ),
        update(serde_json::json!({"sessionUpdate": "config_option_update", "configOptions": []})),
        Event::Closed,
    ] {
        older.update(cx, |older, cx| {
            older.apply(event, cx);
            assert_eq!(older.touched(), before);
            assert!(newer.touched() > older.touched());
        });
    }
    older.read_with(cx, |older, _| {
        assert_eq!(older.title, "A title");
        assert_eq!(older.usage.unwrap().used, 1);
    });
}

#[gpui::test]
fn agent_events_preserve_user_submission_order(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new();
    let chat = cx.new(|_| scratch.chat());
    for event in [
        update(
            serde_json::json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hello"}}),
        ),
        update(
            serde_json::json!({"sessionUpdate": "agent_thought_chunk", "content": {"type": "text", "text": "thinking"}}),
        ),
        update(
            serde_json::json!({"sessionUpdate": "tool_call", "toolCallId": "tool", "title": "Read"}),
        ),
        update(
            serde_json::json!({"sessionUpdate": "tool_call_update", "toolCallId": "tool", "status": "completed"}),
        ),
        update(serde_json::json!({"sessionUpdate": "plan", "entries": []})),
        Event::TurnDone(Ok(StopReason::EndTurn)),
    ] {
        chat.update(cx, |chat, cx| {
            chat.updated = UNIX_EPOCH;
            chat.apply(event, cx);
            assert_eq!(chat.touched(), 0);
        });
    }
}

#[gpui::test]
fn queued_submission_updates_order_immediately_and_survives_restore(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new();
    let chat = cx.new(|_| scratch.chat());
    let submitted = chat.update(cx, |chat, cx| {
        chat.record = None;
        chat.items.push(ChatItem::User("previous prompt".into()));
        chat.streaming = true;
        chat.send("next prompt".into(), cx);
        assert!(chat.touched() > 1000);
        assert_eq!(chat.queue.front().map(String::as_str), Some("next prompt"));
        let submitted = chat.updated;
        chat.apply(Event::TurnDone(Ok(StopReason::EndTurn)), cx);
        assert_eq!(chat.updated, submitted);
        submitted
    });
    cx.run_until_parked();
    let stored = fs::Project::new(&scratch.0).sessions().pop().unwrap();
    let restored = ChatSession::restore(
        2,
        artifact::location::Location::from(scratch.0.clone()),
        agent("/bin/false", &[]),
        stored,
    );
    let touched = chat.read_with(cx, |chat, _| chat.touched());
    assert_eq!(restored.touched() / 1000, touched / 1000);
    assert_eq!(chat.read_with(cx, |chat, _| chat.updated), submitted);
}

#[gpui::test]
fn archived_sessions_ignore_events_and_queued_prompts(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new();
    let chat = cx.new(|_| scratch.chat());
    chat.update(cx, |chat, cx| {
        chat.queue.push_back("queued".into());
        chat.close(cx);
        let before = chat.touched();
        chat.send("late prompt".into(), cx);
        chat.drain(cx);
        chat.apply(
            update(serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "late answer"}
            })),
            cx,
        );
        chat.apply(Event::Closed, cx);
        assert!(chat.closed);
        assert!(matches!(chat.connection, Connection::Idle));
        assert!(chat.queue.is_empty());
        assert!(chat.items.is_empty());
        assert_eq!(chat.touched(), before);
    });
}

async fn wait_for_close(events: &mut acp::Events) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(Event::Closed) | None => return,
                _ => {}
            }
        }
    })
    .await
    .expect("agent process survived closing the session");
}

#[test]
fn cancelling_startup_terminates_an_agent_stuck_in_initialize() {
    acp::runtime().block_on(async {
        let entry = agent("/bin/sh", &["-c", "echo ready >&2; exec sleep 60"]);
        let (tx, mut events) = acp::channel();
        let pending = PendingConnection(acp::runtime().spawn(async move {
            Session::spawn(
                &entry,
                Launch::new(artifact::location::Location::from(std::env::temp_dir())),
                tx,
            )
            .await
        }));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(Event::Stderr(line)) = events.recv().await {
                    assert_eq!(line, "ready");
                    break;
                }
            }
        })
        .await
        .expect("agent did not start");
        drop(pending);
        wait_for_close(&mut events).await;
    });
}

#[gpui::test]
fn archiving_a_connected_session_terminates_its_agent(cx: &mut gpui::TestAppContext) {
    const AGENT: &str = r#"
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    if 'id' not in req:
        continue
    if req['method'] == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}, 'authMethods': []}
    elif req['method'] == 'session/new':
        result = {'sessionId': 'test'}
    else:
        result = {}
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;
    let scratch = Scratch::new();
    let (session, mut events) = acp::runtime().block_on(async {
        let (tx, events) = acp::channel();
        let session = tokio::time::timeout(
            Duration::from_secs(5),
            Session::spawn(
                &agent("/usr/bin/python3", &["-u", "-c", AGENT]),
                Launch::new(artifact::location::Location::from(scratch.0.clone())),
                tx,
            ),
        )
        .await
        .expect("agent did not connect")
        .unwrap();
        (session, events)
    });
    let chat = cx.new(|_| scratch.chat());
    chat.update(cx, |chat, cx| {
        chat.connection = Connection::Live(Box::new(session));
        chat.send("first prompt".into(), cx);
        assert!(chat.touched() > 1000);
    });
    // The first prompt goes once the session has its number.
    cx.run_until_parked();
    chat.update(cx, |chat, cx| {
        chat.send("queued prompt".into(), cx);
        let submitted = chat.updated;
        chat.apply(Event::TurnDone(Ok(StopReason::EndTurn)), cx);
        assert!(chat.queue.is_empty());
        assert_eq!(
            chat.updated, submitted,
            "dispatch must not reorder sessions"
        );
        chat.close(cx);
        assert!(chat.closed);
        assert!(!chat.live());
    });
    acp::runtime().block_on(wait_for_close(&mut events));
}

#[gpui::test]
fn archived_history_unloads_and_metadata_edits_preserve_the_disk_transcript(
    cx: &mut gpui::TestAppContext,
) {
    let scratch = Scratch::new();
    let chat = cx.new(|_| scratch.chat());
    chat.update(cx, |chat, cx| {
        chat.record = None;
        chat.items.push(ChatItem::User("Keep this message".into()));
        chat.items.push(ChatItem::Agent("And this response".into()));
        chat.draft = "unfinished draft".into();
        chat.closed = true;
        drop(chat.mint_record(cx));
    });
    cx.run_until_parked();
    chat.update(cx, |chat, cx| {
        futures::executor::block_on(fs::Project::new(&scratch.0).save_session(&chat.to_record()))
            .unwrap();
        let stored = fs::Project::new(&scratch.0)
            .session(chat.record.as_deref().unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(stored).unwrap(),
            serde_json::to_value(chat.to_record()).unwrap()
        );
        chat.unload_history();
        assert!(chat.history_unloaded);
        assert!(chat.items.is_empty());
        assert!(chat.draft.is_empty());
        assert!(!chat.unsaid());
        chat.name = Some("Renamed archive".into());
        chat.flush(cx);
        chat.unload_history();
        let saved = fs::Project::new(&scratch.0)
            .session(chat.record.as_deref().unwrap())
            .unwrap();
        assert_eq!(saved.items.len(), 2);
        assert_eq!(saved.draft, "unfinished draft");
        assert_eq!(saved.name.as_deref(), Some("Renamed archive"));
        assert!(chat.load_history());
        assert!(chat.items.len() >= 2);
        assert_eq!(chat.draft, "unfinished draft");
    });
}

#[gpui::test]
fn missing_archive_cannot_be_overwritten_with_an_empty_transcript(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new();
    let chat = cx.new(|_| scratch.chat());
    chat.update(cx, |chat, cx| {
        chat.record = None;
        chat.items.push(ChatItem::User("Persisted message".into()));
        chat.closed = true;
        drop(chat.mint_record(cx));
    });
    cx.run_until_parked();
    let store = fs::Project::new(&scratch.0);
    chat.update(cx, |chat, cx| {
        futures::executor::block_on(store.save_session(&chat.to_record())).unwrap();
        chat.unload_history();
        futures::executor::block_on(store.remove_session(chat.record.as_deref().unwrap())).unwrap();
        assert!(!chat.load_history());
        chat.flush(cx);
    });
    cx.run_until_parked();
    assert!(store.sessions().is_empty());
}
