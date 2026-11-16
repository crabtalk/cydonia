use artifact::session::mode::Mode;
use cydonia_gui::model::{
    identity::{
        Identity,
        launch::{Creation, Interactive},
    },
    session_preferences::Choices,
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
