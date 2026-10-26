use cydonia_acp::proto::SessionConfigOptionValue;
use cydonia_gui::model::{
    identity::{Avatar, Identity, Shelf, Store as _},
    session_preferences::Choices,
};
use std::path::PathBuf;

fn choices() -> Choices {
    let mut choices = Choices {
        mode: Some("plan".into()),
        ..Choices::default()
    };
    choices.config.insert(
        "model".into(),
        SessionConfigOptionValue::ValueId {
            value: "opus".into(),
        },
    );
    choices.config.insert(
        "thinking".into(),
        SessionConfigOptionValue::Boolean { value: true },
    );
    choices
}

#[test]
fn a_handle_is_the_name_lowercased_with_runs_of_anything_else_one_dash() {
    assert_eq!(Identity::handle_for("Alice"), "alice");
    assert_eq!(
        Identity::handle_for("  Claude Code (Opus)! "),
        "claude-code-opus"
    );
    assert_eq!(Identity::handle_for("--x--"), "x");
    assert_eq!(Identity::handle_for("…"), "agent");
}

#[test]
fn a_new_identity_mints_a_v7_id_and_reads_back_whole() {
    let made = Shelf::create("Alice", PathBuf::from("/repos/a"), "claude-acp", choices()).unwrap();
    assert_eq!(made.handle, "alice");
    let id = uuid::Uuid::parse_str(&made.id).unwrap();
    assert_eq!(id.get_version_num(), 7);
    assert_eq!(made.avatar, Avatar::from(id));
    assert_eq!(Shelf::all(), vec![made]);
}

#[test]
fn a_taken_handle_is_suffixed() {
    let first = Shelf::create("Alice", PathBuf::from("/repos/a"), "claude-acp", choices()).unwrap();
    let second = Shelf::create("alice", PathBuf::from("/repos/b"), "codex-acp", choices()).unwrap();
    assert_eq!(first.handle, "alice");
    assert_eq!(second.handle, "alice-2");
    assert_ne!(first.id, second.id);
}

#[test]
fn renaming_moves_the_identity_and_keeps_its_id() {
    let alice = Shelf::create("Alice", PathBuf::from("/repos/a"), "claude-acp", choices()).unwrap();
    Shelf::create("Bob", PathBuf::from("/repos/a"), "claude-acp", choices()).unwrap();
    assert!(!Shelf::rename("alice", "bob"));
    assert!(Shelf::rename("alice", "ally"));
    assert!(!Shelf::holds("alice"));
    let moved = Shelf::all()
        .into_iter()
        .find(|identity| identity.handle == "ally")
        .unwrap();
    assert_eq!(moved.id, alice.id);
}

#[test]
fn removing_frees_the_handle() {
    Shelf::create("Alice", PathBuf::from("/repos/a"), "claude-acp", choices()).unwrap();
    Shelf::remove("alice");
    assert!(!Shelf::holds("alice"));
    assert!(Shelf::all().is_empty());
}

#[test]
fn an_avatar_is_written_as_hex_and_rerolls_to_another() {
    let avatar = Avatar(u64::MAX);
    assert_eq!(String::from(avatar), "ffffffffffffffff");
    assert_eq!(Avatar::try_from(String::from(avatar)), Ok(avatar));
    assert_ne!(Avatar::random(), Avatar::random());
}
