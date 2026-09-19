use cydonia_prompts::{self as prompts, resources};
use std::path::Path;

#[test]
fn all_delivery_modes_include_the_shared_workspace_instructions() {
    for available in [false, true] {
        let session = prompts::session_context(Path::new("/projects/my project"), available);
        assert!(session.contains(prompts::workspace()));
        assert!(session.contains("/projects/my project"));
        for bound in [false, true] {
            assert!(prompts::tool_context(bound).contains(prompts::workspace()));
        }
    }
}

#[test]
fn catalog_delivery_never_embeds_full_resource_content() {
    let session = prompts::session_context(Path::new("/project"), true);
    let tools = prompts::tool_context(true);
    assert!(session.contains(&prompts::resource_catalog()));
    assert!(tools.contains(&prompts::resource_catalog()));
    let unavailable = prompts::session_context(Path::new("/project"), false);
    for skill in resources::list() {
        assert!(!session.contains(skill.content));
        assert!(!tools.contains(skill.content));
        assert!(!unavailable.contains(skill.content));
    }
    assert!(!unavailable.contains(&prompts::resource_catalog()));
    assert!(unavailable.contains("MCP connection is unavailable"));
}

#[test]
fn tool_context_distinguishes_project_binding() {
    let bound = prompts::tool_context(true);
    assert!(bound.contains("project bound to this connection"));
    // A bound caller is told it can still name another project, which is the
    // half of the binding a session would otherwise never try.
    assert!(bound.contains("unless a call names"));
    assert!(prompts::tool_context(false).contains("project's directory path"));
}

/// Every caller that can reach the board tools is told to tag what it is
/// working on — and, the two halves that are easy to leave out, to clear the
/// tag when it answers and to tag again when the same card comes back. A
/// status nobody clears is worse than none at all, and a rule that only fires
/// the first time a card is named fires almost never.
#[test]
fn callers_with_tools_are_told_to_tag_and_untag_cards() {
    for context in [
        prompts::session_context(Path::new("/project"), true),
        prompts::tool_context(true),
        prompts::tool_context(false),
    ] {
        assert!(context.contains("board_set_card_status"), "{context}");
        assert!(
            context.contains("A request that names a card is a card you are working"),
            "{context}",
        );
        assert!(
            context.contains("clear it with none when you answer"),
            "{context}"
        );
        assert!(
            context.contains("however often one card comes back"),
            "{context}"
        );
    }
    // Nothing is said to a caller with no tools to say it about.
    let unavailable = prompts::session_context(Path::new("/project"), false);
    assert!(!unavailable.contains("board_set_card_status"));
}
