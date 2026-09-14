//! What an install does to a session that was opened before it.

use cydonia::model::{settings, workspace::readopt};

fn agent(name: &str, command: &str) -> settings::Agent {
    settings::Agent {
        name: name.to_owned(),
        id: None,
        command: command.to_owned(),
        args: Vec::new(),
        env: Default::default(),
    }
}

/// The placeholder a session restored without its agent is holding: a name to
/// show, and nothing to start.
fn placeholder(name: &str) -> settings::Agent {
    agent(name, "")
}

/// The install is the whole fix: the file names claude now, so the session
/// opened on it takes the command it was missing and has somewhere to send
/// again.
#[test]
fn an_installed_agent_reaches_the_session_that_named_it() {
    let mut held = placeholder("claude");
    readopt(&[agent("claude", "claude-code-acp")], &mut held);
    assert_eq!(held.command, "claude-code-acp");
}

/// An entry edited in the file is the one a resume should spawn, not the copy
/// taken when the session was opened.
#[test]
fn a_rewritten_command_is_the_one_the_session_carries() {
    let mut held = agent("claude", "old-binary");
    readopt(&[agent("claude", "new-binary")], &mut held);
    assert_eq!(held.command, "new-binary");
}

/// A session whose agent the file still does not name is the stranded one.
/// Nothing is invented for it — the notice says so instead.
#[test]
fn an_agent_the_file_does_not_name_is_left_as_it_was() {
    let mut held = placeholder("claude");
    readopt(&[agent("codex", "codex-acp")], &mut held);
    assert_eq!(held.name, "claude");
    assert!(
        held.command.is_empty(),
        "nothing to start is still the answer"
    );
}

/// The whole of the bug this rule exists for: the registry renamed `claude` to
/// `Claude Agent`, and every session opened under the old name read as
/// stranded with the agent installed and sitting in the file under the new one.
#[test]
fn a_renamed_agent_is_still_the_one_the_session_runs_on() {
    let mut installed = agent("Claude Agent", "claude-agent-acp");
    installed.id = Some("claude-acp".into());
    let mut held = placeholder("claude");
    held.id = Some("claude-acp".into());

    readopt(std::slice::from_ref(&installed), &mut held);
    assert_eq!(held.name, "Claude Agent");
    assert_eq!(held.command, "claude-agent-acp");
}

/// A record written before ids were stored has only the name to go on, so the
/// name is still tried — and an entry hand-written into `settings.toml` has no
/// id either.
#[test]
fn a_nameless_id_falls_back_to_the_name() {
    let mut held = placeholder("codex");
    readopt(&[agent("codex", "codex-acp")], &mut held);
    assert_eq!(held.command, "codex-acp");
}

/// The id wins where the two disagree: a name is the publisher's to reuse.
#[test]
fn the_id_is_preferred_over_a_name_that_matches_another_agent() {
    let mut claude = agent("Claude Agent", "claude-agent-acp");
    claude.id = Some("claude-acp".into());
    let mut other = agent("claude", "some-other-binary");
    other.id = Some("unrelated".into());

    let mut held = placeholder("claude");
    held.id = Some("claude-acp".into());
    readopt(&[other, claude], &mut held);
    assert_eq!(held.command, "claude-agent-acp");
}
