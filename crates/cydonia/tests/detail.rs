//! Which permission requests come down to an alert, and which fall to the
//! stack of buttons instead.

use cacp::schema::PermissionOptionKind;
use cydonia::{model::session::Choice, view::detail::alert};

fn choice(id: &str, kind: PermissionOptionKind) -> Choice {
    Choice {
        id: id.to_owned(),
        name: id.to_owned(),
        kind,
    }
}

/// The set every agent sends: two answers, one of them rememberable.
#[test]
fn a_yes_a_no_and_a_forever_is_an_alert() {
    let options = vec![
        choice("yes", PermissionOptionKind::AllowOnce),
        choice("yes-always", PermissionOptionKind::AllowAlways),
        choice("no", PermissionOptionKind::RejectOnce),
    ];
    let (deny, allow) = alert(&options).expect("two sides, all three covered");
    assert_eq!(allow.id(false), "yes");
    assert_eq!(allow.id(true), "yes-always");
    // No always-form to send: the refusal stands for this call either way.
    assert_eq!(deny.id(true), "no");
}

/// An option neither side accounts for is an option the alert would drop.
#[test]
fn a_kind_of_the_agents_own_falls_to_the_stack() {
    let options = vec![
        choice("yes", PermissionOptionKind::AllowOnce),
        choice("no", PermissionOptionKind::RejectOnce),
        choice("edit", PermissionOptionKind::Other("edit_first".into())),
    ];
    assert!(alert(&options).is_none());
}

/// So is a second option of a kind one side has already taken.
#[test]
fn a_repeated_kind_falls_to_the_stack() {
    let options = vec![
        choice("yes", PermissionOptionKind::AllowOnce),
        choice("yes-too", PermissionOptionKind::AllowOnce),
        choice("no", PermissionOptionKind::RejectOnce),
    ];
    assert!(alert(&options).is_none());
}

/// An alert needs both answers — a lone side has nothing to sit opposite.
#[test]
fn one_sided_falls_to_the_stack() {
    let options = vec![
        choice("yes", PermissionOptionKind::AllowOnce),
        choice("yes-always", PermissionOptionKind::AllowAlways),
    ];
    assert!(alert(&options).is_none());
}
