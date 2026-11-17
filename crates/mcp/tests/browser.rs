use cydonia_mcp::{
    Server,
    proto::Request,
    rail::{self, Act, Level, Target},
    tool::{Answer, Image},
    tools,
};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
mod common;

#[test]
fn screenshot_returns_image_content_for_the_requested_tab() {
    let scratch = common::Scratch::new("browser-screenshot");
    rail::set_open(vec![scratch.path().to_path_buf()]);
    let project = scratch.path().to_path_buf();
    rail::install_browser(move |request| {
        assert_eq!(request.project, project);
        assert_eq!(request.tab, Some(42));
        assert_eq!(request.act, Act::Screenshot);
        let mut answer = Answer::said("tab 42: https://example.com");
        answer.images.push(Image {
            data: "cGl4ZWxz".into(),
            mime_type: "image/png".into(),
        });
        request.reply.send(Ok(answer)).unwrap();
    });
    let server = Server::new().mount(tools::browser::looking());
    let response = server
        .handle(
            &Request {
                id: Some(json!(1)),
                method: "tools/call".into(),
                params: Some(json!({"name": "browser_screenshot", "arguments": {"tab": 42}})),
            },
            Some(scratch.path()),
        )
        .unwrap();
    assert!(response.error.is_none());
    assert_eq!(
        response.result.unwrap(),
        json!({"content": [
            {"type": "text", "text": "tab 42: https://example.com"},
            {"type": "image", "data": "cGl4ZWxz", "mimeType": "image/png"}
        ]})
    );
}

#[test]
fn screenshot_uses_the_front_tab_and_reports_capture_failures() {
    let scratch = common::Scratch::new("browser-screenshot-error");
    rail::set_open(vec![scratch.path().to_path_buf()]);
    rail::install_browser(|request| {
        assert_eq!(request.tab, None);
        assert_eq!(request.act, Act::Screenshot);
        request
            .reply
            .send(Err("page snapshot timed out".into()))
            .unwrap();
    });
    let response = Server::new()
        .mount(tools::browser::looking())
        .handle(
            &Request {
                id: Some(json!(1)),
                method: "tools/call".into(),
                params: Some(json!({"name": "browser_screenshot"})),
            },
            Some(scratch.path()),
        )
        .unwrap();
    assert!(response.error.is_none());
    assert_eq!(
        response.result.unwrap(),
        json!({
            "content": [{"type": "text", "text": "page snapshot timed out"}],
            "isError": true
        })
    );
}

#[test]
fn debugging_tools_route_expressions_selectors_and_console_filters() {
    let scratch = common::Scratch::new("browser-debugging");
    rail::set_open(vec![scratch.path().to_path_buf()]);
    let cases = [
        (
            "browser_evaluate",
            json!({"script": "Promise.resolve({ready: true})"}),
            Act::Evaluate("Promise.resolve({ready: true})".into()),
        ),
        (
            "browser_inspect",
            json!({"selector": "#hidden-panel"}),
            Act::Inspect(Target::Selector("#hidden-panel".into())),
        ),
        (
            "browser_inspect",
            json!({"element": 0}),
            Act::Inspect(Target::Element(0)),
        ),
        (
            "browser_console",
            json!({"levels": ["warn", "error"], "since": 27}),
            Act::Console {
                levels: vec![Level::Warn, Level::Error],
                since: 27,
            },
        ),
        (
            "browser_console",
            json!({}),
            Act::Console {
                levels: Vec::new(),
                since: 0,
            },
        ),
    ];
    let expected = Arc::new(Mutex::new(
        cases
            .iter()
            .map(|(_, _, act)| act.clone())
            .collect::<VecDeque<_>>(),
    ));
    let pending = expected.clone();
    rail::install_browser(move |request| {
        assert_eq!(request.tab, Some(7));
        assert_eq!(request.act, pending.lock().unwrap().pop_front().unwrap());
        request
            .reply
            .send(Ok(Answer::said("debug result")))
            .unwrap();
    });
    let server = Server::new().mount(&tools::browser::TOOLS);
    for (name, mut arguments, _) in cases {
        arguments["tab"] = json!(7);
        assert_eq!(
            common::said(server.call(name, arguments, Some(scratch.path()))),
            "debug result"
        );
    }
    assert!(expected.lock().unwrap().is_empty());
}

#[test]
fn debugging_tools_reject_ambiguous_targets_and_invalid_filters() {
    let server = Server::new().mount(&tools::browser::TOOLS);
    for (name, arguments) in [
        ("browser_inspect", json!({})),
        (
            "browser_inspect",
            json!({"element": 1, "selector": "button"}),
        ),
        ("browser_inspect", json!({"selector": " "})),
        ("browser_inspect", json!({"element": -1})),
        ("browser_console", json!({"since": -1})),
        ("browser_console", json!({"since": "12"})),
        ("browser_console", json!({"levels": ["warning"]})),
        ("browser_console", json!({"levels": "error"})),
        ("browser_evaluate", json!({"script": "\n "})),
    ] {
        assert!(!common::invalid(server.call(name, arguments, None)).is_empty());
    }
}
