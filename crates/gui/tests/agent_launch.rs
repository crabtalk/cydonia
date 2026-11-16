use artifact::session::mode::Mode;
#[cfg(feature = "desktop")]
use cydonia_acp::agents::program::Program;
use cydonia_acp::proto::SessionConfigOptionValue;
use cydonia_gui::model::{
    identity::{
        Identity,
        launch::{Creation, Interactive},
    },
    session_preferences::Choices,
    settings::Agent,
};
use std::path::PathBuf;

#[test]
fn creation_policy_respects_explicit_choices_and_defaults_without_a_picker() {
    for (preference, fallback) in [
        (Creation::Gui, Mode::Gui),
        (Creation::Terminal, Mode::Terminal),
        (Creation::Choose, Mode::Gui),
    ] {
        assert_eq!(preference.resolve(None), fallback);
        for explicit in [Mode::Gui, Mode::Terminal] {
            assert_eq!(preference.resolve(Some(explicit)), explicit);
        }
    }
}

#[test]
fn old_agents_read_as_gui_without_rewriting_their_settings() {
    let old = r#"
id = "old-agent"
name = "Claude"
workspace = "/repos/a"
acp = "claude-acp"
avatar = "0000000000000001"
"#;
    let agent: Identity = toml::from_str(old).unwrap();
    assert_eq!(agent.creation, Creation::Gui);
    assert_eq!(agent.terminal, None);
}

#[test]
fn interactive_launch_roundtrips_separately_from_acp_choices() {
    let mut agent = Identity::new(
        "Claude",
        PathBuf::from("/repos/a"),
        "claude-acp",
        Choices::default(),
    );
    agent.creation = Creation::Choose;
    agent.terminal = Some(Interactive {
        command: "claude".into(),
        args: vec!["--verbose".into()],
        env: [("CUSTOM".into(), "value".into())].into(),
    });
    let restored: Identity = toml::from_str(&toml::to_string(&agent).unwrap()).unwrap();
    assert_eq!(restored, agent);
    assert_eq!(restored.acp, "claude-acp");
}

#[test]
fn terminal_launch_resolves_adapter_programs_without_acp_arguments_or_environment() {
    for (id, command) in [
        ("codex-acp", "codex"),
        ("claude-acp", "claude"),
        ("pi-acp", "pi"),
        ("antigravity-acp", "agy"),
    ] {
        let agent = Agent {
            name: "Renamed program".into(),
            id: Some(id.into()),
            command: "/installed/acp-adapter".into(),
            args: vec!["--acp".into()],
            env: [("ACP_ONLY".into(), "1".into())].into(),
        };
        let launch = Interactive::resolve(&agent, &Choices::default(), None).unwrap();
        assert_eq!(launch.command, command);
        assert!(launch.args.is_empty());
        assert!(launch.env.is_empty());
    }
}

#[test]
fn native_terminal_launch_reuses_installed_cli_and_removes_acp_mode() {
    for (id, command, args, terminal_args) in [
        ("gemini", "/installed/.bin/gemini", vec!["--acp"], vec![]),
        (
            "gemini",
            "npx",
            vec!["-y", "@google/gemini-cli@0.68.0", "--experimental-acp"],
            vec!["-y", "@google/gemini-cli@0.68.0"],
        ),
        (
            "cursor",
            "/installed/dist-package/cursor-agent",
            vec!["acp"],
            vec![],
        ),
        ("opencode", "/installed/opencode", vec!["acp"], vec![]),
        (
            "opencode-v2",
            "/installed/opencode.exe",
            vec!["acp"],
            vec![],
        ),
    ] {
        let agent = Agent {
            name: "Renamed program".into(),
            id: Some(id.into()),
            command: command.into(),
            args: args.into_iter().map(String::from).collect(),
            env: [("CUSTOM".into(), "value".into())].into(),
        };
        let launch = Interactive::resolve(&agent, &Choices::default(), None).unwrap();
        assert_eq!(launch.command, command);
        assert_eq!(launch.args, terminal_args);
        assert_eq!(launch.env, agent.env);
    }
}

#[test]
fn adapter_packages_resolve_without_registry_ids() {
    for (package, command, args) in [
        (
            "@agentclientprotocol/claude-agent-acp@0.107.2",
            "claude",
            vec![],
        ),
        ("@agentclientprotocol/codex-acp@0.10.0", "codex", vec![]),
        ("pi-acp@0.0.34", "pi", vec![]),
        (
            "@google/gemini-cli@0.68.0",
            "npx",
            vec!["-y", "@google/gemini-cli@0.68.0"],
        ),
    ] {
        let agent = Agent {
            name: "Renamed program".into(),
            id: None,
            command: "npx".into(),
            args: vec!["-y".into(), package.into(), "--acp".into()],
            env: Default::default(),
        };
        let launch = Interactive::resolve(&agent, &Choices::default(), None).unwrap();
        assert_eq!(launch.command, command);
        assert_eq!(launch.args, args);
    }
}

#[test]
fn known_executable_paths_resolve_without_registry_ids_and_receive_the_selected_model() {
    for command in [
        "/custom/claude",
        "/custom/codex",
        "/custom/gemini",
        "/custom/agent",
        "/custom/cursor-agent",
        "/custom/opencode",
        "/custom/pi",
        "/custom/opencode.exe",
        "/custom/agy",
    ] {
        let agent = Agent {
            name: "Renamed program".into(),
            id: None,
            command: command.into(),
            args: Vec::new(),
            env: [("CUSTOM".into(), "value".into())].into(),
        };
        let choices = Choices {
            config: [(
                "model".into(),
                SessionConfigOptionValue::ValueId {
                    value: "chosen-model".into(),
                },
            )]
            .into(),
            ..Choices::default()
        };
        let launch = Interactive::resolve(&agent, &choices, None).unwrap();
        assert_eq!(launch.command, command);
        assert_eq!(launch.args, ["--model", "chosen-model"]);
        assert_eq!(launch.env, agent.env);
    }
}

#[test]
fn antigravity_acp_uses_agy_with_the_selected_model_without_claiming_skills_support() {
    let agent = Agent {
        name: "Antigravity".into(),
        id: Some("antigravity-acp".into()),
        command: "/installed/agy_acp_server.par".into(),
        args: vec!["--uid=1234".into()],
        env: Default::default(),
    };
    let choices = Choices {
        config: [(
            "model".into(),
            SessionConfigOptionValue::ValueId {
                value: "chosen-model".into(),
            },
        )]
        .into(),
        ..Choices::default()
    };
    let launch = Interactive::resolve(&agent, &choices, None).unwrap();
    assert_eq!(launch.command, "agy");
    assert_eq!(launch.args, ["--model", "chosen-model"]);
    assert!(agent.harness().is_none());
}

#[test]
fn codex_tui_uses_selected_model_and_reasoning_with_optional_overrides() {
    let agent = Agent {
        name: "Codex".into(),
        id: None,
        command: "npx".into(),
        args: vec!["@agentclientprotocol/codex-acp@0.10.0".into()],
        env: Default::default(),
    };
    let choices = Choices {
        config: [
            (
                "model".into(),
                SessionConfigOptionValue::ValueId {
                    value: "chosen-model".into(),
                },
            ),
            (
                "reasoning_effort".into(),
                SessionConfigOptionValue::ValueId {
                    value: "high".into(),
                },
            ),
        ]
        .into(),
        ..Choices::default()
    };
    let overrides: Interactive =
        toml::from_str("args = ['--no-alt-screen']\n[env]\nCUSTOM = 'value'").unwrap();
    let launch = Interactive::resolve(&agent, &choices, Some(&overrides)).unwrap();
    assert_eq!(launch.command, "codex");
    assert_eq!(
        launch.args,
        [
            "--model",
            "chosen-model",
            "--config",
            "model_reasoning_effort=\"high\"",
            "--no-alt-screen"
        ]
    );
    assert_eq!(launch.env, overrides.env);
    let explicit = Interactive {
        command: "/custom/codex".into(),
        args: vec!["--model".into(), "override-model".into()],
        env: Default::default(),
    };
    let launch = Interactive::resolve(&agent, &choices, Some(&explicit)).unwrap();
    assert_eq!(launch.command, explicit.command);
    assert_eq!(
        launch.args,
        [
            "--config",
            "model_reasoning_effort=\"high\"",
            "--model",
            "override-model"
        ]
    );
}

#[test]
fn custom_agents_require_an_explicit_terminal_command_and_preserve_it() {
    let agent = Agent {
        name: "Custom".into(),
        id: None,
        command: "acp-server".into(),
        args: Vec::new(),
        env: Default::default(),
    };
    assert!(Interactive::resolve(&agent, &Choices::default(), None).is_none());
    let overrides = Interactive {
        command: "/custom/interactive".into(),
        args: vec!["--interactive".into()],
        env: [("CUSTOM".into(), "value".into())].into(),
    };
    assert_eq!(
        Interactive::resolve(&agent, &Choices::default(), Some(&overrides)),
        Some(overrides)
    );
}

#[cfg(feature = "desktop")]
#[test]
fn codex_mcp_is_project_bound_launch_configuration_without_a_session_id() {
    let mut launch = Interactive {
        command: "codex".into(),
        args: vec!["--no-alt-screen".into()],
        env: Default::default(),
    };
    let path = PathBuf::from("/repos/project with spaces/文档");
    launch.attach_mcp(Program::Codex, "http://127.0.0.1:1234/mcp", &path);
    assert_eq!(launch.args[0], "--config");
    assert_eq!(launch.args[2], "--config");
    let config: toml::Value =
        toml::from_str(&format!("{}\n{}", launch.args[1], launch.args[3])).unwrap();
    let server = &config["mcp_servers"]["cydonia"];
    assert_eq!(server["url"].as_str(), Some("http://127.0.0.1:1234/mcp"));
    let headers = server["http_headers"].as_table().unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(
        headers[mcp::http::PROJECT].as_str(),
        Some(mcp::http::encoded(&path).as_str())
    );
    assert_eq!(launch.args.last().unwrap(), "--no-alt-screen");
    assert_eq!(launch.env.len(), 2);
    assert!(
        launch.env["NO_PROXY"]
            .split(',')
            .any(|host| host == "127.0.0.1")
    );
}

#[cfg(feature = "desktop")]
#[test]
fn claude_mcp_is_additive_launch_configuration_and_other_agents_are_unchanged() {
    let original = Interactive {
        command: "custom-wrapper".into(),
        args: vec![
            "--mcp-config".into(),
            r#"{"mcpServers":{"existing":{"type":"http","url":"https://example.com/mcp"}}}"#.into(),
        ],
        env: Default::default(),
    };
    let mut launch = original.clone();
    let path = PathBuf::from("/repos/a");
    launch.attach_mcp(Program::Claude, "http://127.0.0.1:1234/mcp", &path);
    let config: serde_json::Value = serde_json::from_str(&launch.args[1]).unwrap();
    assert_eq!(launch.args[0], "--mcp-config");
    assert_eq!(
        config["mcpServers"]["cydonia"],
        serde_json::json!({
            "type": "http", "url": "http://127.0.0.1:1234/mcp", "headers": { (mcp::http::PROJECT): mcp::http::encoded(&path) }
        })
    );
    assert_eq!(&launch.args[2..], original.args);
    assert!(!launch.args.iter().any(|arg| arg == "--strict-mcp-config"));
    assert_eq!(launch.env.len(), 2);
    assert!(
        launch.env["no_proxy"]
            .split(',')
            .any(|host| host == "127.0.0.1")
    );
    for program in [
        Program::Gemini,
        Program::Cursor,
        Program::OpenCode,
        Program::Pi,
        Program::Antigravity,
    ] {
        let mut launch = original.clone();
        launch.attach_mcp(program, "http://127.0.0.1:1234/mcp", &path);
        assert_eq!(launch, original);
    }
}

#[test]
fn initial_tasks_reach_the_native_interactive_prompt_without_bootstrap_text() {
    use cydonia_acp::agents::program::Program;
    for (program, option) in [
        (Program::Codex, "--"),
        (Program::Claude, "--"),
        (Program::Cursor, "--"),
        (Program::Pi, "--"),
        (Program::Gemini, "--prompt-interactive"),
        (Program::Antigravity, "-i"),
        (Program::OpenCode, "--prompt"),
    ] {
        let mut launch = Interactive {
            command: "agent".into(),
            args: Vec::new(),
            env: Default::default(),
        };
        let task = "--review 'quoted paths'\nand explain the result";
        launch.prompt(Some(program), task.to_owned());
        assert_eq!(launch.args, [option, task]);
        assert!(launch.env.is_empty());
    }
}
