//! cydonia's MCP for an agent on the server's machine: the test is the agent,
//! talking stdio MCP to `cydonia-server mcp`, which joins a daemon run in
//! this process over its unix socket, which relays to a client attached
//! through a proxy and answering with cydonia's MCP server.

#![cfg(unix)]

mod common;

use std::{
    io::{BufRead as _, BufReader, Write as _},
    os::unix::net::UnixStream,
    path::Path,
    process::{Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

use artifact::{
    location::Location,
    project::{Project as _, fs},
};
use common::Scratch;
use cupola::{
    Client, Connection,
    daemon::Daemon,
    proxy::{self, Launch},
};
use cydonia_server::{
    Command, STATE_DIR, Services,
    mcp::{Binding, Mcp},
};
use serde_json::Value;
use tokio::runtime::Runtime;

const PATIENCE: Duration = Duration::from_secs(10);

#[test]
fn an_agent_lists_and_calls_tools_through_the_daemon() {
    let home = Scratch::short("mcp");
    let project = home.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    futures::executor::block_on(fs::Project::new(&project).create_board("Roadmap", "ROAD"))
        .unwrap();
    mcp::rail::install(|_| {});
    mcp::rail::set_open(vec![Location::from(project.as_path())]);

    let runtime = Runtime::new().unwrap();
    let state = home.path().join(STATE_DIR);
    runtime.spawn({
        let state = state.clone();
        async move {
            let _ = Services::run(&Daemon::at(state)).await;
        }
    });
    eventually("the daemon is listening", || {
        std::fs::read_dir(&state).is_ok_and(|entries| entries.count() >= 3)
    });

    let tools = mcp::Server::new().mount(&mcp::tools::board::TOOLS);
    let (client, runner) = Client::new();
    Mcp::serve(&client, move |request| {
        let binding = &request.binding;
        tools
            .message(
                &request.message,
                binding.project.as_ref(),
                binding.session.as_deref(),
            )
            .text()
    });
    runtime.spawn(runner);
    let _proxy = attach(&runtime, &state, &client);

    let binding = Binding {
        project: Some(Location::from(project.as_path())),
        session: None,
    };
    let mut agent = Process::new(env!("CARGO_BIN_EXE_cydonia-server"))
        .args(Command::Mcp(binding).args())
        .env("HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = agent.stdin.take().unwrap();
    for message in [
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"board_list","arguments":{}}}"#,
        "not json",
    ] {
        writeln!(input, "{message}").unwrap();
    }
    drop(input);
    let answers: Vec<Value> = BufReader::new(agent.stdout.take().unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(agent.wait().unwrap().success());

    assert_eq!(answers.len(), 3, "{answers:?}");
    let answer = |id: Value| {
        answers
            .iter()
            .find(|answer| answer["id"] == id)
            .unwrap_or_else(|| panic!("no answer for {id}: {answers:?}"))
    };
    let listed = answer(1.into())["result"]["tools"].as_array().unwrap();
    assert!(listed.iter().any(|tool| tool["name"] == "board_list"));
    let called = answer(2.into())["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(called.contains("Roadmap"), "{called}");
    assert_eq!(answer(Value::Null)["error"]["code"], -32700);
}

#[test]
fn joining_with_no_daemon_fails() {
    let home = Scratch::short("mcp-none");
    let output = Process::new(env!("CARGO_BIN_EXE_cydonia-server"))
        .args(Command::Mcp(Binding::default()).args())
        .env("HOME", home.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

/// Attaches `client` to the daemon in `state` through a proxy on a thread,
/// over a socket pair, as ssh's stdio would carry it.
fn attach(
    runtime: &Runtime,
    state: &Path,
    client: &Client,
) -> thread::JoinHandle<Result<(), proxy::Error>> {
    let (near, far) = UnixStream::pair().unwrap();
    let state = state.to_owned();
    let proxy = thread::spawn(move || {
        let input = far.try_clone().unwrap();
        Daemon::at(state).proxy(Launch::Reconnect, input, far)
    });
    let client = client.clone();
    runtime.spawn(async move {
        let near = async_net::unix::UnixStream::try_from(near).unwrap();
        let _ = client.attach(Connection::new(near.clone(), near)).await;
    });
    proxy
}

fn eventually(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < PATIENCE, "{what}");
        thread::sleep(Duration::from_millis(20));
    }
}
