use cacp_agents::{Distribution, package_name, registry};

#[test]
fn package_names_keep_their_scope() {
    assert_eq!(
        package_name("@agentclientprotocol/claude-agent-acp@0.68.0"),
        "@agentclientprotocol/claude-agent-acp"
    );
    assert_eq!(package_name("@scope/name"), "@scope/name");
    assert_eq!(package_name("plain@1.2.3"), "plain");
    assert_eq!(package_name("plain"), "plain");
}

#[test]
fn parses_the_published_shape() {
    let registry = registry::parse(
        r#"{"version":"1.0.0","agents":[
            {"id":"claude-acp","name":"Claude Agent","version":"0.68.0",
             "distribution":{"npx":{"package":"@agentclientprotocol/claude-agent-acp@0.68.0"}}},
            {"id":"gemini","name":"Gemini CLI","version":"0.55.1",
             "distribution":{"npx":{"package":"@google/gemini-cli@0.55.1","args":["--acp"]}}},
            {"id":"opencode","name":"OpenCode","version":"1.0.0",
             "distribution":{"binary":{"darwin-aarch64":{"archive":"https://example.invalid/x.zip"}}}}
        ]}"#,
    )
    .expect("parses");
    assert_eq!(registry.agents.len(), 3);
    assert!(matches!(
        &registry.agents[1].distribution,
        Distribution::Npm { args, .. } if args == &["--acp"]
    ));
    assert!(!registry.agents[2].installable());
}
