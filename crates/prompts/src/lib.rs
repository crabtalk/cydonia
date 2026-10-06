//! Shared instructions and built-in resources, independent of agent transports.

pub mod resources;

use std::path::Path;

const WORKSPACE: &str = include_str!("../instructions/workspace.md");
const ARTIFACTS: &str = include_str!("../instructions/artifacts.md");

pub fn workspace() -> &'static str {
    WORKSPACE.trim()
}

/// The catalog, less the resources named in `hidden` — those of surfaces
/// switched off.
pub fn resource_catalog(hidden: &[&str]) -> String {
    format!(
        "Available Cydonia resources:\n{}",
        resources::catalog(hidden)
    )
}

/// Instructions for callers with or without a bound project.
pub fn tool_context(bound: bool, hidden: &[&str]) -> String {
    let project = if bound {
        "Project tools work in the project bound to this connection unless a call names \
another one. A named project must be one cydonia has open; the project argument takes its \
directory path."
    } else {
        "Project tools take the project's directory path. Resources are independent of projects."
    };
    format!(
        "{}\n\n{project}\n\n{}\n\n{}",
        workspace(),
        ARTIFACTS.trim(),
        resource_catalog(hidden)
    )
}

/// Current session state alongside static instructions, refreshed each turn.
///
/// `session` is the session's entry number; without one the context names no
/// session.
pub fn session_context(
    cwd: &Path,
    session: Option<u64>,
    mcp_available: bool,
    hidden: &[&str],
) -> String {
    let mut context = format!(
        "Cydonia session context\n{}\n\nCurrent project: {}\n",
        workspace(),
        cwd.display(),
    );
    if let Some(number) = session {
        context.push_str(&format!("Current session: #{number}\n"));
    }
    context.push('\n');
    if mcp_available {
        context.push_str(ARTIFACTS.trim());
        context.push_str("\n\n");
        context.push_str(&resource_catalog(hidden));
    } else {
        context.push_str("Cydonia's MCP connection is unavailable. Required reference documents cannot be loaded. Report this limitation before tasks that depend on Cydonia tools or reference documents; do not guess their behavior.");
    }
    context
}
