use super::*;
use artifact::project::fs;
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
        ChatSession::restore(1, self.0.clone(), agent("/bin/false", &[]), record)
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

#[test]
fn background_events_do_not_move_an_older_session_above_a_new_one() {
    let scratch = Scratch::new();
    let mut older = scratch.chat();
    let mut newer = scratch.chat();
    newer.updated = SystemTime::now();
    let before = older.touched();
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
        older.apply(event);
        assert_eq!(older.touched(), before);
        assert!(newer.touched() > older.touched());
    }
    assert_eq!(older.title, "A title");
    assert_eq!(older.usage.unwrap().used, 1);
}

#[test]
fn agent_events_preserve_user_submission_order() {
    let scratch = Scratch::new();
    let mut chat = scratch.chat();
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
        chat.updated = UNIX_EPOCH;
        chat.apply(event);
        assert_eq!(chat.touched(), 0);
    }
}

#[test]
fn queued_submission_updates_order_immediately_and_survives_restore() {
    let scratch = Scratch::new();
    let mut chat = scratch.chat();
    chat.record = None;
    chat.items.push(ChatItem::User("previous prompt".into()));
    chat.streaming = true;
    chat.send("next prompt".into());
    assert!(chat.touched() > 1000);
    assert_eq!(chat.queue.front().map(String::as_str), Some("next prompt"));
    let submitted = chat.updated;
    chat.apply(Event::TurnDone(Ok(StopReason::EndTurn)));
    assert_eq!(chat.updated, submitted);
    let stored = fs::Project::new(&scratch.0).sessions().pop().unwrap();
    let restored = ChatSession::restore(2, scratch.0.clone(), agent("/bin/false", &[]), stored);
    assert_eq!(restored.touched() / 1000, chat.touched() / 1000);
}

#[test]
fn archived_sessions_ignore_events_and_queued_prompts() {
    let scratch = Scratch::new();
    let mut chat = scratch.chat();
    chat.queue.push_back("queued".into());
    chat.close();
    let before = chat.touched();
    chat.send("late prompt".into());
    chat.drain();
    chat.apply(update(serde_json::json!({
        "sessionUpdate": "agent_message_chunk",
        "content": {"type": "text", "text": "late answer"}
    })));
    chat.apply(Event::Closed);
    assert!(chat.closed);
    assert!(matches!(chat.connection, Connection::Idle));
    assert!(chat.queue.is_empty());
    assert!(chat.items.is_empty());
    assert_eq!(chat.touched(), before);
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
            Session::spawn(&entry, Launch::new(std::env::temp_dir()), tx).await
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

#[test]
fn archiving_a_connected_session_terminates_its_agent() {
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
    acp::runtime().block_on(async {
        let (tx, mut events) = acp::channel();
        let session = tokio::time::timeout(
            Duration::from_secs(5),
            Session::spawn(
                &agent("/usr/bin/python3", &["-u", "-c", AGENT]),
                Launch::new(scratch.0.clone()),
                tx,
            ),
        )
        .await
        .expect("agent did not connect")
        .unwrap();
        let mut chat = scratch.chat();
        chat.connection = Connection::Live(Box::new(session));
        chat.send("first prompt".into());
        assert!(chat.touched() > 1000);
        chat.send("queued prompt".into());
        let submitted = chat.updated;
        chat.apply(Event::TurnDone(Ok(StopReason::EndTurn)));
        assert!(chat.queue.is_empty());
        assert_eq!(
            chat.updated, submitted,
            "dispatch must not reorder sessions"
        );
        chat.close();
        assert!(chat.closed);
        assert!(!chat.live());
        wait_for_close(&mut events).await;
    });
}

#[test]
fn archived_history_unloads_and_metadata_edits_preserve_the_disk_transcript() {
    let scratch = Scratch::new();
    let mut chat = scratch.chat();
    chat.record = None;
    chat.items.push(ChatItem::User("Keep this message".into()));
    chat.items.push(ChatItem::Agent("And this response".into()));
    chat.draft = "unfinished draft".into();
    chat.closed = true;
    chat.mint_record();
    fs::Project::new(&scratch.0)
        .save_session(&chat.to_record())
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
    chat.flush();
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
}

#[test]
fn missing_archive_cannot_be_overwritten_with_an_empty_transcript() {
    let scratch = Scratch::new();
    let mut chat = scratch.chat();
    chat.record = None;
    chat.items.push(ChatItem::User("Persisted message".into()));
    chat.closed = true;
    chat.mint_record();
    fs::Project::new(&scratch.0)
        .save_session(&chat.to_record())
        .unwrap();
    chat.unload_history();
    let store = fs::Project::new(&scratch.0);
    store
        .remove_session(chat.record.as_deref().unwrap())
        .unwrap();
    assert!(!chat.load_history());
    chat.flush();
    assert!(store.sessions().is_empty());
}
