use cydonia_mcp::{Server, proto::Request, tools};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

fn request(server: &Server, method: &str, params: Value) -> Value {
    let request = Request {
        id: Some(json!(1)),
        method: method.to_owned(),
        params: Some(params),
    };
    server.handle(&request, None).unwrap().result.unwrap()
}

#[test]
fn skills_are_discoverable_and_readable_without_a_project_in_read_only_mode() {
    let server = Server::new()
        .mount(&tools::skill::TOOLS)
        .writable(Arc::new(AtomicBool::new(false)));
    let hello = request(&server, "initialize", json!({}));
    assert!(
        hello["instructions"]
            .as_str()
            .unwrap()
            .contains(&prompts::skills::catalog())
    );
    let listed = request(&server, "tools/list", json!({}));
    let tool = &listed["tools"][0];
    assert_eq!(tool["name"], "skill_read");
    assert_eq!(tool["inputSchema"]["required"], json!(["name"]));
    assert!(tool["inputSchema"]["properties"].get("project").is_none());
    let result = request(
        &server,
        "tools/call",
        json!({"name": "skill_read", "arguments": {"name": "cydonia-markdown"}}),
    );
    assert_eq!(
        result["content"][0]["text"],
        prompts::skills::read("cydonia-markdown").unwrap().content
    );
}

#[test]
fn unknown_skills_return_a_recoverable_error_and_catalog() {
    let server = Server::new().mount(&tools::skill::TOOLS);
    let result = request(
        &server,
        "tools/call",
        json!({"name": "skill_read", "arguments": {"name": "../../secret"}}),
    );
    assert_eq!(result["isError"], true);
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(&prompts::skills::catalog())
    );
}

#[test]
fn unmounted_skills_are_not_advertised() {
    let hello = request(&Server::new(), "initialize", json!({}));
    assert!(
        !hello["instructions"]
            .as_str()
            .unwrap()
            .contains("skill_read")
    );
}
