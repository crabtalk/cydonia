//! The agent's own output, on its way into the block that carries it: what
//! the block is headed with, and what is dropped when it will not all fit.

use artifact::session::chat::ChatItem;
use cydonia_gui::model::session::{command_line, nothing_said, trim_front};
use cydonia_gui::model::settings::Agent;
use std::collections::BTreeMap;

/// A block of whatever the agent process wrote to its stderr.
fn process(output: &str) -> ChatItem {
    ChatItem::Process {
        command: "claude-agent-acp".into(),
        output: output.into(),
    }
}

fn agent(command: &str, args: &[&str]) -> Agent {
    Agent {
        name: "test".into(),
        id: None,
        command: command.into(),
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        env: BTreeMap::new(),
    }
}

/// The head of the block is what was run, spelled the way it was run —
/// otherwise the output is attributed to a command nobody typed.
#[test]
fn the_block_is_headed_with_the_command_line() {
    assert_eq!(
        command_line(&agent(
            "npx",
            &["-y", "@agentclientprotocol/codex-acp@1.8.0"]
        )),
        "npx -y @agentclientprotocol/codex-acp@1.8.0"
    );
}

/// An installed agent takes no arguments at all, and the head is the path on
/// its own rather than a path with a trailing space.
#[test]
fn a_command_with_no_arguments_stands_alone() {
    assert_eq!(
        command_line(&agent("/opt/acp/claude", &[])),
        "/opt/acp/claude"
    );
}

/// Under the ceiling nothing moves. The common case, and the one where
/// touching the string at all would be waste.
#[test]
fn output_within_the_ceiling_is_left_alone() {
    let mut text = String::from("one\ntwo\nthree");
    trim_front(&mut text, 1024);
    assert_eq!(text, "one\ntwo\nthree");
}

/// Over it, whole lines go off the front — never half of one, because half a
/// line of output reads as a different error than the one printed.
#[test]
fn whole_lines_go_off_the_front() {
    let mut text = String::from("aaaa\nbbbb\ncccc\ndddd");
    trim_front(&mut text, 10);
    assert_eq!(text, "cccc\ndddd");
    assert!(text.len() <= 10);
}

/// The tail is the half kept, because what a process said last is what it
/// said about dying.
#[test]
fn the_tail_is_what_survives() {
    let mut text = (0..500).map(|n| format!("line {n}\n")).collect::<String>();
    trim_front(&mut text, 64);
    assert!(text.ends_with("line 499\n"));
    assert!(!text.contains("line 0\n"));
}

/// One line longer than the whole budget has no newline to cut at. Its tail
/// still matters, so it is cut into — on a boundary it survives, which a
/// multi-byte character is what makes a question.
#[test]
fn a_single_oversized_line_is_cut_on_a_char_boundary() {
    let mut text = "é".repeat(100);
    trim_front(&mut text, 31);
    assert!(text.len() <= 31);
    assert!(text.chars().all(|c| c == 'é'));
}

/// Trimming twice is trimming once — a block that is appended to line after
/// line must settle at the ceiling rather than creep past it.
#[test]
fn repeated_trims_hold_the_ceiling() {
    let mut text = String::new();
    for n in 0..2000 {
        text.push_str(&format!("line {n}\n"));
        trim_front(&mut text, 256);
        assert!(text.len() <= 256, "grew to {} at {n}", text.len());
    }
    assert!(text.ends_with("line 1999\n"));
}

/// And the whole mechanism, on the launch it exists for: a command that
/// writes to stderr and dies without ever speaking protocol.
///
/// This is the `#!/usr/bin/env node` shim that found no node. What it printed
/// used to go to `/dev/null` and the session said only that the connection
/// closed; the point of holding the channel outside the launch is that the
/// line survives the error.
#[test]
fn a_launch_that_dies_still_hands_back_what_it_printed() {
    use cydonia_gui::agent::acp::{self, Event, Launch, Session};
    use std::time::Duration;

    let entry = agent(
        "/bin/sh",
        &[
            "-c",
            "echo 'env: node: No such file or directory' >&2; exit 127",
        ],
    );
    let (tx, mut events) = acp::channel();
    let (opened, printed) = acp::runtime().block_on(async move {
        let opened = Session::spawn(&entry, Launch::new(std::env::temp_dir()), tx).await;
        // The stderr reader is a task of its own, so wait for the line rather
        // than for a clock. The timeout is only there to fail rather than hang.
        let printed = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match events.recv().await {
                    Some(Event::Stderr(line)) => return Some(line),
                    Some(_) => continue,
                    None => return None,
                }
            }
        })
        .await;
        (opened, printed)
    });

    assert!(opened.is_err(), "a process that exits 127 opened a session");
    assert_eq!(
        printed.expect("timed out waiting for the line"),
        Some("env: node: No such file or directory".to_owned())
    );
}

/// A session an agent has only cleared its throat in is a session nothing has
/// been said in.
///
/// The agent is spawned when the session opens, not when the first message is
/// sent, so anything it writes to stderr on the way up — a deprecation
/// warning, a runtime's banner — was landing in the transcript before anybody
/// had typed a word. That took the empty state away from a new session and
/// minted a `.cydonia/` file for a conversation nobody had.
#[test]
fn an_agents_startup_noise_is_not_somebody_talking() {
    assert!(nothing_said(&[]));
    assert!(nothing_said(&[process("(node:1) DeprecationWarning: ...")]));
    assert!(nothing_said(&[
        process("first line"),
        process("second line"),
    ]));
}

/// What the app says for itself does count: a failed connection has to show,
/// and the stderr above it is the reason.
#[test]
fn a_notice_is_worth_the_transcript_and_the_file() {
    assert!(!nothing_said(&[
        process("npm warn exec ..."),
        ChatItem::Notice {
            text: "connection failed".into(),
            failed: true,
        },
    ]));
}

/// And so, obviously, does anything anybody typed.
#[test]
fn a_message_is_the_session_beginning() {
    assert!(!nothing_said(&[ChatItem::User("what's up".into())]));
    assert!(!nothing_said(&[ChatItem::Agent("not much".into())]));
}
