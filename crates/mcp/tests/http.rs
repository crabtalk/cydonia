//! The door, over a real socket: what it refuses, and what it answers.

mod common;

use common::Scratch;
use cydonia_mcp::{Server, http, tools};
use std::{
    io::{Read as _, Write as _},
    net::TcpStream,
    sync::Arc,
};

/// The one check there is. A process running as this user can write every file
/// these tools write; a page in a browser cannot, and is the only caller a
/// loopback port lets in that could not otherwise get there.
#[test]
fn a_browser_is_turned_away() {
    let (_scratch, _runtime, url) = door("origin");
    let list = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

    let refused = send(&url, list, &[("Origin", "https://example.com")]);
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");

    let answer = send(&url, list, &[]);
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("board_list"), "{answer}");
}

/// A notification is answered by not answering, which over HTTP is the status
/// that says so.
#[test]
fn a_notification_is_accepted_and_not_answered() {
    let (_scratch, _runtime, url) = door("accepted");
    let hello = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let answer = send(&url, hello, &[]);
    assert!(answer.starts_with("HTTP/1.1 202"), "{answer}");
}

/// A call names its own project, and the door holds none.
#[test]
fn a_call_reaches_the_directory_it_names() {
    let (scratch, _runtime, url) = door("directory");
    scratch.store_create("Roadmap", "ROAD").expect("a board");
    let call = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"board_list","arguments":{{"project":"{}"}}}}}}"#,
        scratch.path().display()
    );
    let answer = send(&url, &call, &[]);
    assert!(answer.contains("Roadmap"), "{answer}");
}

/// The GET half of Streamable HTTP is the server's own stream, and this one
/// pushes nothing.
#[test]
fn there_is_no_stream_to_open() {
    let (_scratch, _runtime, url) = door("no-stream");
    let answer = request(&url, "GET", "", &[]);
    assert!(answer.starts_with("HTTP/1.1 405"), "{answer}");
}

/// A door on a port the system picked, and the runtime holding it up — both
/// returned, because dropping either closes it.
fn door(name: &str) -> (Scratch, tokio::runtime::Runtime, String) {
    let scratch = Scratch::new(name);
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let server = Arc::new(Server::new().mount(&tools::board::TOOLS));
    let door = runtime
        .block_on(http::open_at(0, server))
        .expect("a free port");
    let url = door.url().to_owned();
    // Held by the runtime for the rest of the test: the listener lives in a
    // task, and letting the handle drop here would close it before the first
    // call.
    std::mem::forget(door);
    (scratch, runtime, url)
}

fn send(url: &str, body: &str, headers: &[(&str, &str)]) -> String {
    request(url, "POST", body, headers)
}

#[test]
fn an_external_client_can_discover_and_read_resources() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = Arc::new(Server::new());
    let door = runtime.block_on(http::open_at(0, server)).unwrap();
    let list = send(
        door.url(),
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
        &[],
    );
    assert!(list.starts_with("HTTP/1.1 200"));
    assert!(list.contains("cydonia://resources/markdown"));
    let read = send(
        door.url(),
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"cydonia://resources/markdown"}}"#,
        &[],
    );
    assert!(read.starts_with("HTTP/1.1 200"));
    assert!(read.contains("text/markdown"));
    assert!(read.contains("# Cydonia Markdown"));
}

/// One request, written by hand. A client here would be a dependency for the
/// sake of four lines of it.
fn request(url: &str, method: &str, body: &str, headers: &[(&str, &str)]) -> String {
    let rest = url.strip_prefix("http://").expect("a loopback url");
    let (authority, path) = rest.split_once('/').expect("a path");
    let mut stream = TcpStream::connect(authority).expect("the door");
    let mut request = format!(
        "{method} /{path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).expect("the request");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer
}

/// Something else on the pinned port is answered by taking the next one. A
/// second cydonia is the usual reason, and the first must not be what stops it
/// starting.
#[test]
fn a_taken_port_is_stepped_past() {
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let first = runtime
        .block_on(http::open(Arc::new(Server::new())))
        .expect("the pinned port, or one after it");
    let second = runtime
        .block_on(http::open(Arc::new(Server::new())))
        .expect("the one after that");
    assert_ne!(first.url(), second.url());
    assert!(
        first.url().starts_with("http://127.0.0.1:"),
        "{}",
        first.url()
    );
}

/// A caller opened in a project is told which on the way in, so its tools take
/// no directory at all — an argument a model has to supply is one it can
/// supply wrongly, about something already known here.
#[test]
fn a_bound_caller_never_names_its_project() {
    let scratch = Scratch::new("bound");
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let server = Arc::new(Server::new().mount(&tools::article::TOOLS));
    let door = runtime
        .block_on(http::open_at(0, server))
        .expect("a free port");
    let bound = http::encoded(scratch.path());
    let list = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

    let told = send(door.url(), list, &[(http::PROJECT, &bound)]);
    assert!(!told.contains("\"project\""), "{told}");
    let loose = send(door.url(), list, &[]);
    assert!(loose.contains("\"project\""), "{loose}");

    // And a call lands in the bound directory without being given one.
    let call = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"article_list","arguments":{}}}"#;
    let answer = send(door.url(), call, &[(http::PROJECT, &bound)]);
    assert!(answer.contains("no articles"), "{answer}");
}

/// A project is whatever somebody called their directory, and a header value
/// is ASCII.
#[test]
fn a_project_survives_the_header_it_rides_on() {
    let awkward = std::path::Path::new("/Users/tianyi/文档/my project");
    let there_and_back = http::encoded(awkward);
    assert!(there_and_back.is_ascii(), "{there_and_back}");

    let scratch = Scratch::new("encoded");
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let server = Arc::new(Server::new().mount(&tools::article::TOOLS));
    let door = runtime
        .block_on(http::open_at(0, server))
        .expect("a free port");
    // The round trip is what the door does with it, so it is asked through the
    // door: a path it decoded wrongly is a directory that is not there.
    let call = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"article_list","arguments":{}}}"#;
    let answer = send(
        door.url(),
        call,
        &[(http::PROJECT, &http::encoded(scratch.path()))],
    );
    assert!(answer.contains("no articles"), "{answer}");
}
