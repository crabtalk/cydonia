//! Which permission requests come down to an alert, and which fall to the
//! stack of buttons instead.

use cacp::schema::PermissionOptionKind;
use cydonia::{
    model::session::Choice,
    view::detail::{adrift, adrift_line, agent_missing, alert},
};

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

// ── a session with no agent to reach ─────────────────────────────

/// The pane says which agent is missing, by the name the session was filed
/// under — not "no agent", which is a state and not the thing to fix.
#[test]
fn the_missing_agent_is_named() {
    assert_eq!(agent_missing(Some("claude")), "claude is not installed");
}

/// Except where a session was asked for and never opened. There is no agent
/// to name then, and naming none is the honest title.
#[test]
fn a_session_never_opened_names_no_agent() {
    assert_eq!(agent_missing(None), "No agent installed");
}

/// With another agent on the machine, opening a session on it is the shorter
/// way back than a download, so that is what the line offers first.
#[test]
fn another_agent_installed_is_the_shorter_way_out() {
    let line = adrift(Some("claude"), true);
    assert!(line.contains("Open a session"), "{line}");
    assert!(line.contains("install"), "the download is still offered");
}

/// With none there is nothing to open a session on, and offering it would be
/// pointing at a menu with no rows.
#[test]
fn nothing_installed_offers_no_session_to_open() {
    let line = adrift(Some("claude"), false);
    assert!(!line.contains("Open a session"), "{line}");
}

/// A ⌘N on a machine with nothing installed has no session behind it, so the
/// line is about the install and says nothing of what is stranded.
#[test]
fn a_first_session_is_offered_the_install_alone() {
    let line = adrift(None, false);
    assert!(line.contains("Install one"), "{line}");
    assert!(!line.contains("either"), "nothing was stranded: {line}");
}

/// The strip under a transcript carries no button, so the line has to say
/// where the install is.
#[test]
fn the_strip_names_where_to_go() {
    for others in [true, false] {
        let line = adrift_line("claude", others);
        assert!(line.starts_with("claude is not installed"), "{line}");
        assert!(line.contains("Settings › Agents"), "{line}");
    }
}

mod panel_sizing {
    use cydonia::view::detail::{panel_beside, panel_width};

    /// A width nobody chose is half the window.
    #[test]
    fn an_unsized_panel_takes_a_share_of_the_window() {
        let about = |width: f32, expected: f32| {
            assert!(
                (width - expected).abs() < 0.01,
                "{width} is not about {expected}"
            );
        };
        about(panel_width(None, 1800.), 900.);
        about(panel_width(None, 1200.), 600.);
        about(panel_width(None, 600.), 300.);
    }

    /// The panel never takes more than half of a window it is standing in.
    #[test]
    fn an_unsized_panel_leaves_the_chat_the_larger_half() {
        for available in [560., 700., 900., 1200., 1800.] {
            let width = panel_width(None, available);
            assert!(
                width <= available / 2.,
                "{width} of {available} is more than half"
            );
        }
    }

    /// A width somebody dragged is kept as far as it fits.
    #[test]
    fn a_dragged_width_is_kept() {
        assert_eq!(panel_width(Some(700.), 1800.), 700.);
        assert_eq!(panel_width(Some(300.), 1800.), 300.);
        // Wider than the chat, down to what leaves the chat its minimum.
        assert_eq!(panel_width(Some(700.), 1000.), 700.);
        assert_eq!(panel_width(Some(700.), 800.), 560.);
    }

    /// A dragged width always leaves the chat its minimum.
    #[test]
    fn a_dragged_width_leaves_the_chat_its_minimum() {
        for available in [560., 700., 900., 1200., 1800.] {
            let width = panel_width(Some(1713.6), available);
            assert!(
                available - width >= 240.,
                "{width} of {available} leaves the chat too little"
            );
        }
    }

    /// Below the two minimums together the column is not split at all: the
    /// panel covers it instead.
    #[test]
    fn a_narrow_window_does_not_stand_them_side_by_side() {
        assert!(panel_beside(520.));
        assert!(panel_beside(1200.));
        assert!(!panel_beside(519.));
        assert!(!panel_beside(400.));
    }
}
