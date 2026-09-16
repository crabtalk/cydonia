use artifact::session::chat::ChatItem;
use cydonia::{
    agent::acp::{self, Event, Launch, Session},
    model::settings,
};
use std::time::Duration;

const AGENT: &str = r#"
import json, sys
for line in sys.stdin:
    req = json.loads(line)
    method = req.get('method')
    if 'id' not in req:
        continue
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {'loadSession': True}, 'authMethods': []}
    elif method == 'session/new':
        result = {'sessionId': 'fork'}
    elif method == 'session/load':
        if req['params']['sessionId'] == 'gone':
            print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'error': {'code': -32002, 'message': 'missing'}}), flush=True)
            continue
        result = {}
    elif method == 'session/prompt':
        assert req['params']['sessionId'] == 'fork'
        print(json.dumps({'jsonrpc': '2.0', 'method': 'session/update', 'params': {
            'sessionId': 'fork', 'update': {'sessionUpdate': 'agent_message_chunk', 'content': {
                'type': 'text', 'text': json.dumps(req['params']['prompt'])}}}}), flush=True)
        result = {'stopReason': 'end_turn'}
    else:
        print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'error': {'code': -32601, 'message': 'unsupported'}}), flush=True)
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

fn agent() -> settings::Agent {
    settings::Agent {
        name: "fake".into(),
        id: None,
        command: "/usr/bin/python3".into(),
        args: vec!["-u".into(), "-c".into(), AGENT.into()],
        env: Default::default(),
    }
}

async fn answer(events: &mut acp::Events) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut text = String::new();
        loop {
            match events.recv().await.expect("agent closed") {
                Event::Update(cacp::schema::SessionUpdate::AgentMessageChunk(chunk)) => {
                    if let cacp::schema::ContentBlock::Text(body) = chunk.content {
                        text.push_str(&body.text);
                    }
                }
                Event::TurnDone(result) => {
                    result.unwrap();
                    return text;
                }
                _ => {}
            }
        }
    })
    .await
    .expect("prompt timed out")
}

#[test]
fn fork_context_is_sent_once_and_completed_forks_resume_without_reimporting() {
    acp::runtime().block_on(async {
        let history = acp::history(&[
            ChatItem::User("unique prior question".into()),
            ChatItem::Agent("unique prior answer".into()),
        ]);
        let (tx, mut events) = acp::channel();
        let session = Session::spawn(
            &agent(),
            Launch {
                history: Some(history.clone()),
                history_pending: true,
                ..Launch::new(std::env::temp_dir())
            },
            tx,
        )
        .await
        .unwrap();
        assert!(!session.loaded);
        assert!(
            events.try_recv().is_err(),
            "opening a fork must not run a turn"
        );
        session.prompt("edited question");
        let text = answer(&mut events).await;
        assert!(text.contains("unique prior question"));
        assert!(text.contains("unique prior answer"));
        assert!(text.contains("edited question"));
        session.prompt("next question");
        let text = answer(&mut events).await;
        assert!(!text.contains("unique prior question"));
        drop(session);

        let (tx, mut events) = acp::channel();
        let loaded = Session::spawn(
            &agent(),
            Launch {
                previous: Some("fork".into()),
                history: Some(history),
                history_pending: false,
                ..Launch::new(std::env::temp_dir())
            },
            tx,
        )
        .await
        .unwrap();
        assert!(loaded.loaded);
        loaded.prompt("after reopening");
        assert!(!answer(&mut events).await.contains("unique prior question"));
    });
}

#[test]
fn tool_history_is_context_in_order() {
    let items = vec![
        ChatItem::User("question".into()),
        ChatItem::Process {
            command: "echo example".into(),
            output: "example".into(),
        },
        ChatItem::Agent("answer".into()),
    ];
    let history = acp::history(&items);
    assert_eq!(history.len(), items.len());
    assert_eq!(history[1].role, cacp::client::HistoryRole::Tool);
    assert_eq!(
        history[1].content,
        vec![cacp::schema::ContentBlock::from("echo example\nexample")]
    );
}

#[test]
fn unsent_fork_reimports_after_loading_and_missing_agent_state_uses_saved_history() {
    acp::runtime().block_on(async {
        for (previous, pending, loaded) in [("fork", true, true), ("gone", false, false)] {
            let (tx, mut events) = acp::channel();
            let session = Session::spawn(
                &agent(),
                Launch {
                    previous: Some(previous.into()),
                    history: Some(acp::history(&[ChatItem::Agent("preserved context".into())])),
                    history_pending: pending,
                    ..Launch::new(std::env::temp_dir())
                },
                tx,
            )
            .await
            .unwrap();
            assert_eq!(session.loaded, loaded);
            session.prompt("continue");
            assert!(answer(&mut events).await.contains("preserved context"));
        }
    });
}

#[test]
fn an_empty_fork_is_saved_with_its_draft_before_any_prompt() {
    use artifact::{
        project::{Project as _, fs::Project},
        session::record::Record,
    };
    use cydonia::model::session::ChatSession;
    let path = std::env::temp_dir().join(format!(
        "cydonia-fork-{}-{}",
        std::process::id(),
        artifact::stamp::now()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let record: Record = serde_json::from_value(serde_json::json!({
        "id": "source", "agent": "fake", "title": "Original", "name": null,
        "updated": 1, "items": [{"User": "editable prompt"}]
    }))
    .unwrap();
    let source = ChatSession::restore(1, path.clone(), agent(), record);
    let mut fork = source.fork_at(2, 0).unwrap();
    fork.flush();
    let records = Project::new(&path).sessions();
    assert_eq!(records.len(), 1);
    let saved = records.into_iter().next().unwrap();
    assert_ne!(saved.id, "source");
    let restored = ChatSession::restore(3, path.clone(), agent(), saved);
    assert!(restored.items.is_empty());
    assert_eq!(restored.draft, "editable prompt");
    assert!(restored.fork.unwrap().pending);
    std::fs::remove_dir_all(path).unwrap();
}
