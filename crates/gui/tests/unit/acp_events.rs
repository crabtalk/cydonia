use crate::agent::acp::{self, Bridge, Event};
use cydonia_acp::{
    Client as _,
    proto::{
        self, ContentBlock, ContentChunk, CurrentModeUpdate, ReadTextFileRequest,
        RequestPermissionResponse, SessionNotification, SessionUpdate, StopReason,
        WriteTextFileRequest,
    },
};
use cydonia_acp_events as acp_events;
use std::{fs, time::Duration};

#[test]
fn queued_updates_precede_turn_completion_and_closure() {
    acp::runtime().block_on(async {
        let (client, protocol) = acp_events::channel();
        let (tx, mut events) = acp::channel();
        for text in ["first", "last"] {
            client
                .session_update(SessionNotification {
                    session_id: "session".into(),
                    update: SessionUpdate::AgentMessageChunk(ContentChunk {
                        content: text.into(),
                        message_id: None,
                        meta: None,
                    }),
                    meta: None,
                })
                .await;
        }
        let bridge = Bridge::spawn(protocol, tx.clone());
        bridge.flush().await;
        assert!(tx.send(Event::TurnDone(Ok(StopReason::EndTurn))).is_ok());
        drop(client);
        for expected in ["first", "last"] {
            let Some(Event::Update(SessionUpdate::AgentMessageChunk(chunk))) = events.recv().await
            else {
                panic!("update must precede completion");
            };
            let ContentBlock::Text(text) = chunk.content else {
                panic!("expected text");
            };
            assert_eq!(text.text, expected);
        }
        assert!(matches!(events.recv().await, Some(Event::TurnDone(Ok(_)))));
        assert!(matches!(events.recv().await, Some(Event::Closed)));
        bridge.flush().await;
    });
}

#[test]
fn permission_replies_use_the_protocol_channel() {
    acp::runtime().block_on(async {
        let (client, protocol) = acp_events::channel();
        let (tx, mut events) = acp::channel();
        let _bridge = Bridge::spawn(protocol, tx);
        let request = serde_json::from_value(serde_json::json!({
            "sessionId": "session",
            "toolCall": {"toolCallId": "tool"},
            "options": []
        }))
        .unwrap();
        let call = client.request_permission(request);
        tokio::pin!(call);
        tokio::select! {
            biased;
            _ = &mut call => panic!("permission must wait for the frontend"),
            event = events.recv() => {
                let Some(Event::Permission(_, reply)) = event else {
                    panic!("expected permission request");
                };
                reply.send(RequestPermissionResponse::cancelled());
            }
        }
        assert_eq!(call.await.unwrap(), RequestPermissionResponse::cancelled());
    });
}

#[test]
fn file_requests_are_served_without_the_ui_pump() {
    acp::runtime().block_on(async {
        let (client, protocol) = acp_events::channel();
        let (tx, _events) = acp::channel();
        let _bridge = Bridge::spawn(protocol, tx);
        let path = std::env::temp_dir().join(format!("cydonia-acp-{}", uuid::Uuid::now_v7()));
        client
            .write_text_file(WriteTextFileRequest {
                session_id: "session".into(),
                path: path.clone(),
                content: "one\ntwo\nthree\n".into(),
                meta: None,
            })
            .await
            .unwrap();
        let request = ReadTextFileRequest {
            session_id: "session".into(),
            path: path.clone(),
            line: Some(2),
            limit: Some(1),
            meta: None,
        };
        assert_eq!(
            client
                .read_text_file(request.clone())
                .await
                .unwrap()
                .content,
            "two"
        );
        fs::remove_file(path).unwrap();
        assert!(client.read_text_file(request).await.is_err());
        let error = client
            .ext_request("unsupported".into(), serde_json::Value::Null)
            .await
            .unwrap_err();
        assert_eq!(error.code, proto::METHOD_NOT_FOUND);
    });
}

#[test]
fn dropping_the_ui_declines_requests() {
    acp::runtime().block_on(async {
        let (client, protocol) = acp_events::channel();
        let (tx, events) = acp::channel();
        let _bridge = Bridge::spawn(protocol, tx);
        drop(events);
        let request = serde_json::from_value(serde_json::json!({
            "sessionId": "session",
            "toolCall": {"toolCallId": "tool"},
            "options": []
        }))
        .unwrap();
        let error =
            tokio::time::timeout(Duration::from_secs(2), client.request_permission(request))
                .await
                .unwrap()
                .unwrap_err();
        assert_eq!(error.code, proto::METHOD_NOT_FOUND);
    });
}

#[test]
fn connection_closure_drains_queued_updates() {
    acp::runtime().block_on(async {
        let (client, protocol) = acp_events::channel();
        client
            .session_update(SessionNotification {
                session_id: "session".into(),
                update: SessionUpdate::CurrentModeUpdate(CurrentModeUpdate {
                    current_mode_id: "ask".into(),
                    meta: None,
                }),
                meta: None,
            })
            .await;
        drop(client);
        let (tx, mut events) = acp::channel();
        let bridge = Bridge::spawn(protocol, tx);
        bridge.flush().await;
        assert!(matches!(events.recv().await, Some(Event::Update(_))));
        assert!(matches!(events.recv().await, Some(Event::Closed)));
        assert!(events.recv().await.is_none());
    });
}
