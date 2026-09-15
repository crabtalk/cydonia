use cydonia_prompts::{self as prompts, skills};
use std::path::Path;

#[test]
fn all_delivery_modes_include_the_shared_workspace_instructions() {
    for available in [false, true] {
        let session = prompts::session_context(Path::new("/projects/my project"), available);
        assert!(session.contains(prompts::workspace()));
        assert!(session.contains("/projects/my project"));
        for bound in [false, true] {
            assert!(prompts::tool_context(bound, available).contains(prompts::workspace()));
        }
    }
}

#[test]
fn catalog_delivery_defers_content_and_fallback_embeds_it() {
    let session = prompts::session_context(Path::new("/project"), true);
    let tools = prompts::tool_context(true, true);
    assert!(session.contains(&prompts::skill_instructions()));
    assert!(tools.contains(&prompts::skill_instructions()));
    let fallback = prompts::session_context(Path::new("/project"), false);
    for skill in skills::list() {
        assert!(!session.contains(skill.content));
        assert!(!tools.contains(skill.content));
        assert!(fallback.contains(skill.content));
    }
    assert!(!prompts::tool_context(false, false).contains(&prompts::skill_instructions()));
}

#[test]
fn tool_context_distinguishes_project_binding() {
    assert!(prompts::tool_context(true, true).contains("project bound to this connection"));
    assert!(prompts::tool_context(false, true).contains("project's directory path"));
}
