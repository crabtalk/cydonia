//! cydonia's MCP tools for an agent on another machine, over
//! [`common::Served`]: a call relayed through the `mcp` service names its
//! project by location, and reaches it through the store the app holds for
//! it. The served directory stands in for the host's: the location names a
//! directory that is also on this machine, and that one is left alone.

mod common;

use artifact::{
    location::Location,
    project::{Project as _, fs, remote, store::Store},
};
use common::{Scratch, Served};
use cydonia_server::mcp::{Binding, Mcp, Request};
use serde_json::{Value, json};

/// The relayed message for `tools/call` of `name` with `arguments`, answered.
fn call(served: &Served, project: &Location, name: &str, arguments: Value) -> Value {
    let message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments },
    });
    let request = Request {
        binding: Binding {
            project: Some(project.clone()),
            session: None,
        },
        message: message.to_string(),
    };
    let answer = served
        .runtime
        .block_on(served.server.service::<Mcp>().send(&request))
        .unwrap()
        .expect("a request is answered");
    serde_json::from_str(&answer).unwrap()
}

fn text(answer: &Value) -> &str {
    answer["result"]["content"][0]["text"].as_str().unwrap()
}

/// The tools answering on `served`'s client, the app's rail holding the
/// project at `held`, kept in `store`.
fn answering(served: &Served, held: Location, store: Store) {
    mcp::rail::install(|_| {});
    mcp::rail::set_open(vec![held.clone()]);
    mcp::rail::install_stores(move |location| (*location == held).then(|| store.clone()));
    let tools = mcp::Server::new()
        .mount(&mcp::tools::board::TOOLS)
        .mount(&mcp::tools::article::TOOLS)
        .mount(&mcp::tools::project::TOOLS);
    Mcp::serve(&served.client, move |request| {
        let binding = &request.binding;
        tools
            .message(
                &request.message,
                binding.project.as_ref(),
                binding.session.as_deref(),
            )
            .text()
    });
}

#[test]
fn a_relayed_call_reads_and_writes_the_remote_project_and_not_the_local_one() {
    let host = Scratch::new("tools-host");
    let here = Scratch::new("tools-here");
    let served = Served::new();
    let (project, refresh) = served
        .runtime
        .block_on(remote::Project::open(
            served.client.clone(),
            host.path().to_owned(),
        ))
        .unwrap();
    served.runtime.spawn(refresh);
    let location = Location::parse(&format!("cupola://7{}", here.path().display()));
    answering(&served, location.clone(), Store::Remote(project));

    let made = call(
        &served,
        &location,
        "board_add",
        json!({"name": "Roadmap", "key": "ROAD"}),
    );
    assert!(made["result"]["isError"].is_null(), "{made}");
    let added = call(
        &served,
        &location,
        "article_add",
        json!({"title": "Notes", "text": "# Notes\n"}),
    );
    assert!(added["result"]["isError"].is_null(), "{added}");

    let there = fs::Project::new(host.path());
    assert_eq!(
        there
            .boards()
            .iter()
            .map(|board| board.name.as_str())
            .collect::<Vec<_>>(),
        ["Roadmap"]
    );
    assert_eq!(there.articles()[0].title, "Notes");
    assert!(
        !fs::Project::new(here.path()).cydonia().exists(),
        "the directory of that path on this machine is not written"
    );

    let listed = call(&served, &location, "project_entries", json!({}));
    assert!(text(&listed).contains("Roadmap"), "{listed}");
    assert!(text(&listed).contains("Notes"), "{listed}");
}

#[test]
fn a_call_for_a_project_the_app_does_not_hold_is_refused() {
    let host = Scratch::new("tools-unheld");
    let served = Served::new();
    let held = Location::parse(&format!("cupola://7{}", host.path().display()));
    let store = Store::Fs(fs::Project::new(host.path()));
    answering(&served, held, store);

    let elsewhere = Location::parse(&format!("cupola://8{}", host.path().display()));
    let refused = call(&served, &elsewhere, "board_list", json!({}));
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert!(text(&refused).contains(&elsewhere.to_string()), "{refused}");
    assert!(!fs::Project::new(host.path()).cydonia().exists());
}
