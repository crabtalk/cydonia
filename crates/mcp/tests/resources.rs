use cydonia_mcp::{
    Server,
    proto::{self, Request, Response},
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn call(server: &Server, method: &str, params: Value, project: Option<&Path>) -> Response {
    server
        .handle(
            &Request {
                id: Some(json!(1)),
                method: method.into(),
                params: Some(params),
            },
            project,
        )
        .unwrap()
}

#[test]
fn listed_resources_return_bundled_content() {
    let server = Server::new().writable(Arc::new(AtomicBool::new(false)));
    let init = call(&server, "initialize", json!({}), None).result.unwrap();
    assert_eq!(init["capabilities"]["resources"], json!({}));
    let listing = call(&server, "resources/list", json!({}), None)
        .result
        .unwrap();
    assert!(listing.get("nextCursor").is_none());
    let resources = listing["resources"].as_array().unwrap();
    assert_eq!(resources.len(), prompts::resources::list().len());
    for resource in resources {
        let name = resource["name"].as_str().unwrap();
        let skill = prompts::resources::read(name).unwrap();
        assert_eq!(resource["uri"], skill.uri());
        assert_eq!(resource["mimeType"], "text/markdown");
        assert_eq!(resource["size"], skill.content.len());
        for project in [None, Some(Path::new("/does/not/need/to/exist"))] {
            let read = call(
                &server,
                "resources/read",
                json!({ "uri": resource["uri"] }),
                project,
            )
            .result
            .unwrap();
            assert_eq!(read["contents"][0]["text"], skill.content);
            assert_eq!(read["contents"][0]["uri"], resource["uri"]);
            assert_eq!(read["contents"][0]["mimeType"], "text/markdown");
        }
    }
}

#[test]
fn resource_errors_distinguish_bad_arguments_from_unknown_uris() {
    let server = Server::new();
    for params in [Value::Null, json!({}), json!({"uri": 42})] {
        assert_eq!(
            call(&server, "resources/read", params, None)
                .error
                .unwrap()
                .code,
            proto::INVALID_PARAMS
        );
    }
    for uri in [
        "cydonia://resources/missing",
        "file:///etc/passwd",
        "cydonia://resources/../../secret",
        "cydonia://resources/markdown/extra",
        "cydonia://resources/markdown?other=true",
    ] {
        assert_eq!(
            call(&server, "resources/read", json!({ "uri": uri }), None)
                .error
                .unwrap()
                .code,
            proto::RESOURCE_NOT_FOUND
        );
    }
    assert_eq!(
        call(
            &server,
            "resources/list",
            json!({ "cursor": "unknown" }),
            None
        )
        .error
        .unwrap()
        .code,
        proto::INVALID_PARAMS
    );
}

#[test]
fn static_resources_offer_no_templates_or_subscriptions() {
    let server = Server::new();
    assert_eq!(
        call(&server, "resources/templates/list", json!({}), None)
            .result
            .unwrap(),
        json!({ "resourceTemplates": [] })
    );
    assert_eq!(
        call(
            &server,
            "resources/subscribe",
            json!({ "uri": "cydonia://resources/markdown" }),
            None
        )
        .error
        .unwrap()
        .code,
        proto::METHOD_NOT_FOUND
    );
}

#[test]
fn resources_do_not_require_a_skill_tool() {
    let server = Server::new();
    let init = call(&server, "initialize", json!({}), None).result.unwrap();
    assert_eq!(init["capabilities"]["resources"], json!({}));
    let list = call(&server, "tools/list", json!({}), None).result.unwrap();
    assert!(list["tools"].as_array().unwrap().is_empty());
    assert!(
        server
            .call("skill_read", json!({"name": "markdown"}), None)
            .is_err()
    );
}
