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
    assert!(prompts::tool_context(true).contains("project bound to this connection"));
    assert!(prompts::tool_context(false).contains("project's directory path"));
}
