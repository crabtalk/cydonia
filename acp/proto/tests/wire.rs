//! Wire shapes that are not obvious from the type definitions: tagged enums
//! with an untagged fallback, nested tags, the renames that do not follow the
//! container's rule, and what happens to a shape this revision does not know.

use cydonia_acp_proto as proto;
use serde_json::json;

fn round<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: T,
    expected: serde_json::Value,
) {
    assert_eq!(serde_json::to_value(&value).unwrap(), expected);
    assert_eq!(serde_json::from_value::<T>(expected).unwrap(), value);
}

#[test]
fn auth_method_agent_carries_no_tag() {
    round(
        proto::AuthMethod::Agent(proto::AuthMethodAgent {
            id: "oauth".into(),
            name: "Sign in".into(),
            description: None,
            meta: None,
        }),
        json!({"id": "oauth", "name": "Sign in"}),
    );
}

#[test]
fn auth_method_tags_the_rest() {
    round(
        proto::AuthMethod::Terminal(proto::AuthMethodTerminal {
            id: "tui".into(),
            name: "Terminal".into(),
            description: None,
            args: vec!["login".into()],
            env: Default::default(),
            meta: None,
        }),
        json!({"type": "terminal", "id": "tui", "name": "Terminal", "args": ["login"]}),
    );
}

#[test]
fn mcp_server_acp_is_tagged_and_stdio_still_is_not() {
    round(
        proto::McpServer::Acp(proto::McpServerAcp {
            name: "editor".into(),
            server_id: "s1".into(),
            meta: None,
        }),
        json!({"type": "acp", "name": "editor", "serverId": "s1"}),
    );

    let stdio: proto::McpServer =
        serde_json::from_value(json!({"name": "local", "command": "/bin/srv"})).unwrap();
    assert!(matches!(stdio, proto::McpServer::Stdio(_)));
}

#[test]
fn plan_update_nests_a_tag_inside_the_session_update_tag() {
    round(
        proto::SessionUpdate::PlanUpdate(proto::PlanUpdate {
            plan: proto::PlanUpdateContent::Items(proto::PlanItems {
                plan_id: "p1".into(),
                entries: vec![],
                meta: None,
            }),
            meta: None,
        }),
        json!({
            "sessionUpdate": "plan_update",
            "plan": {"type": "items", "planId": "p1", "entries": []},
        }),
    );

    round(
        proto::SessionUpdate::PlanRemoved(proto::PlanRemoved {
            plan_id: "p1".into(),
            meta: None,
        }),
        json!({"sessionUpdate": "plan_removed", "planId": "p1"}),
    );
}

#[test]
fn nes_suggestions_tag_on_kind_in_camel_case() {
    round(
        proto::NesSuggestion::SearchAndReplace(proto::NesSearchAndReplaceSuggestion {
            id: "s1".into(),
            uri: "file:///a.rs".into(),
            search: "foo".into(),
            replace: "bar".into(),
            is_regex: None,
            meta: None,
        }),
        json!({
            "kind": "searchAndReplace",
            "id": "s1",
            "uri": "file:///a.rs",
            "search": "foo",
            "replace": "bar",
        }),
    );
}

#[test]
fn position_encodings_keep_their_hyphens() {
    round(proto::PositionEncodingKind::Utf16, json!("utf-16"));
    round(proto::PositionEncodingKind::Utf8, json!("utf-8"));
}

#[test]
fn unknown_llm_protocols_survive_as_themselves() {
    round(proto::LlmProtocol::OpenAi, json!("openai"));
    round(proto::LlmProtocol::Other("groq".into()), json!("groq"));
}

#[test]
fn fork_is_advertised_as_a_bare_presence_flag() {
    let capabilities = proto::SessionCapabilities {
        fork: Some(proto::Capability::default()),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(capabilities).unwrap(),
        json!({"fork": {}})
    );
}

#[test]
fn end_of_turn_usage_is_optional() {
    round(
        proto::PromptResponse::new(proto::StopReason::EndTurn),
        json!({"stopReason": "end_turn"}),
    );
    round(
        proto::PromptResponse {
            stop_reason: proto::StopReason::EndTurn,
            usage: Some(proto::Usage {
                total_tokens: 30,
                input_tokens: 10,
                output_tokens: 20,
                ..Default::default()
            }),
            meta: None,
        },
        json!({
            "stopReason": "end_turn",
            "usage": {"totalTokens": 30, "inputTokens": 10, "outputTokens": 20},
        }),
    );
}

#[test]
fn meta_rides_along_untouched() {
    let notification: proto::SessionNotification = serde_json::from_value(json!({
        "sessionId": "s1",
        "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "hi"}},
        "_meta": {"claudeCode": {"parentToolUseId": "t1"}},
    }))
    .unwrap();

    assert_eq!(
        notification.meta.as_ref().unwrap()["claudeCode"]["parentToolUseId"],
        json!("t1")
    );
    assert_eq!(
        serde_json::to_value(&notification).unwrap()["_meta"]["claudeCode"]["parentToolUseId"],
        json!("t1")
    );
}

#[test]
fn meta_survives_a_flattened_neighbour() {
    let update: proto::ToolCallUpdate = serde_json::from_value(json!({
        "toolCallId": "t1",
        "status": "in_progress",
        "_meta": {"claudeCode": {"parentToolUseId": "t0"}},
    }))
    .unwrap();

    assert_eq!(
        update.fields.status,
        Some(proto::ToolCallStatus::InProgress)
    );
    assert!(update.meta.is_some());
    assert_eq!(
        serde_json::to_value(&update).unwrap(),
        json!({
            "toolCallId": "t1",
            "status": "in_progress",
            "_meta": {"claudeCode": {"parentToolUseId": "t0"}},
        })
    );
}

#[test]
fn an_unknown_update_kind_is_kept_whole() {
    let update: proto::SessionUpdate =
        serde_json::from_value(json!({"sessionUpdate": "state_update", "state": "idle"})).unwrap();

    let proto::SessionUpdate::Other(ref other) = update else {
        panic!("expected the catch-all, got {update:?}");
    };
    assert_eq!(other.session_update, "state_update");
    assert_eq!(other.fields["state"], json!("idle"));

    round(
        update,
        json!({"sessionUpdate": "state_update", "state": "idle"}),
    );
}

#[test]
fn an_unknown_content_block_is_kept_whole() {
    round(
        proto::ContentBlock::Other(proto::OtherContentBlock {
            kind: "video".into(),
            fields: [("uri".to_string(), json!("file:///a.mp4"))]
                .into_iter()
                .collect(),
        }),
        json!({"type": "video", "uri": "file:///a.mp4"}),
    );
}

#[test]
fn a_null_request_id_is_a_request_id() {
    round(proto::RequestId::Null, json!(null));
    round(proto::RequestId::Num(7), json!(7));
}

#[test]
fn boxed_annotations_stay_flat_on_the_wire() {
    round(
        proto::ContentBlock::Text(proto::TextContent {
            text: "hi".into(),
            annotations: Some(Box::new(proto::Annotations {
                audience: Some(vec![proto::Role::User]),
                priority: Some(0.5),
                ..Default::default()
            })),
            meta: None,
        }),
        json!({
            "type": "text",
            "text": "hi",
            "annotations": {"audience": ["user"], "priority": 0.5},
        }),
    );
}
