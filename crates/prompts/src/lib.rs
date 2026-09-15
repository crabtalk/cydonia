//! Shared instructions and built-in resources, independent of agent transports.

pub mod resources;

use std::path::Path;

const WORKSPACE: &str = include_str!("../instructions/workspace.md");
const ARTIFACTS: &str = include_str!("../instructions/artifacts.md");

pub fn workspace() -> &'static str {
    WORKSPACE.trim()
}

pub fn resource_catalog() -> String {
    format!("Available Cydonia resources:\n{}", resources::catalog())
}

/// Instructions for callers with or without a bound project.
pub fn tool_context(bound: bool) -> String {
    let project = if bound {
        "Project tools operate on the project bound to this connection."
    } else {
        "Project tools take the project's directory path. Resources are independent of projects."
    };
    format!(
        "{}\n\n{project}\n{}\n\n{}",
        workspace(),
        ARTIFACTS.trim(),
        resource_catalog()
    )
}

/// Current session state alongside static instructions, refreshed each turn.
pub fn session_context(cwd: &Path, mcp_available: bool) -> String {
    let mut context = format!(
        "Cydonia session context\n{}\n\nCurrent project: {}\n\n",
        workspace(),
        cwd.display(),
    );
    if mcp_available {
        context.push_str(ARTIFACTS.trim());
        context.push_str("\n\n");
        context.push_str(&resource_catalog());
    } else {
        context.push_str("Cydonia's MCP connection is unavailable. Required reference documents cannot be loaded. Report this limitation before tasks that depend on Cydonia tools or reference documents; do not guess their behavior.");
    }
    context
}
