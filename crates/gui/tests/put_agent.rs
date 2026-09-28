//! Naming an installed agent in `settings.toml`.

use cydonia_gui::model::settings::{self, Agent};

fn file() -> String {
    std::fs::read_to_string(settings::dir().unwrap().join("settings.toml")).unwrap()
}

/// A generated file names no agents at all, rather than an empty inline list
/// that later has to become `[[agents]]`.
#[test]
fn a_fresh_file_has_no_agents_key() {
    assert!(settings::load().unwrap().agents.is_empty());
    let doc: toml_edit::DocumentMut = file().parse().unwrap();
    assert!(doc.get("agents").is_none());
}

/// A file written before that still says `agents = []`, and the first install
/// has to turn it into a table it can add to.
#[test]
fn the_first_install_lands_beside_an_empty_list() {
    assert!(settings::load().unwrap().agents.is_empty());
    let path = settings::dir().unwrap().join("settings.toml");
    std::fs::write(&path, format!("agents = []\n{}", file())).unwrap();
    let agent = Agent {
        name: "Claude".into(),
        id: Some("claude-acp".into()),
        command: r"C:\Users\me\agents\claude-acp\node_modules\.bin\claude.cmd".into(),
        args: Vec::new(),
        env: Default::default(),
    };
    settings::put_agent(&agent, Some("@x/claude")).unwrap();
    assert!(file().contains("[[agents]]"));
    let agents = settings::load().unwrap().agents;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].command, agent.command);
}
