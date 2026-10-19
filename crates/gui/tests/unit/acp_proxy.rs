use super::{Ours, acp_mcp_servers, loopback_bypass, mcp};
use artifact::location::Location;
use cydonia_acp::proto::{InitializeResponse, McpServer};
use std::path::{Path, PathBuf};

fn stdio(name: &str, remote: bool) -> mcp::McpServer {
    mcp::McpServer {
        name: name.into(),
        enabled: true,
        command: Some(format!("/bin/{name}")),
        args: Vec::new(),
        env: Default::default(),
        url: None,
        id: None,
        remote,
    }
}

#[test]
fn a_remote_agent_gets_the_relay_and_only_the_stdio_servers_marked_remote() {
    let init: InitializeResponse =
        serde_json::from_value(serde_json::json!({"protocolVersion": 1})).unwrap();
    let configured = [stdio("here", false), stdio("there", true)];
    let program = PathBuf::from("/home/me/.cupola/cydonia-server-0.2.3");
    let location = Location::parse("cupola://7/home/me/work");
    let (servers, built_in) = acp_mcp_servers(
        &configured,
        &init,
        &Ours::Relay(program.clone()),
        false,
        &location,
        Some("rec"),
    );
    assert!(built_in);
    let named: Vec<(&str, &Path)> = servers
        .iter()
        .map(|server| match server {
            McpServer::Stdio(stdio) => (stdio.name.as_str(), stdio.command.as_path()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        named,
        [
            ("cydonia", program.as_path()),
            ("there", Path::new("/bin/there"))
        ]
    );
    let McpServer::Stdio(relay) = &servers[0] else {
        unreachable!("matched above");
    };
    let at = relay
        .args
        .iter()
        .position(|arg| arg == "--project")
        .unwrap();
    assert_eq!(
        relay.args[at + 1],
        "cupola://7/home/me/work",
        "the binding names the project by its location, not by its path on the host"
    );
}

#[test]
fn loopback_is_exempt_without_existing_bypasses() {
    assert_eq!(loopback_bypass([None, None]), "localhost,127.0.0.1,::1");
}

#[test]
fn both_proxy_bypass_lists_are_preserved() {
    assert_eq!(
        loopback_bypass([
            Some(" .example.com, localhost, ".into()),
            Some("10.0.0.0/8,localhost".into()),
        ]),
        ".example.com,localhost,10.0.0.0/8,127.0.0.1,::1"
    );
    assert_eq!(
        loopback_bypass([Some("*".into()), None]),
        "*,localhost,127.0.0.1,::1"
    );
}
