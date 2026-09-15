//! Shared instructions and built-in skills, independent of agent transports.

pub mod skills;

use std::path::Path;

const WORKSPACE: &str = include_str!("../instructions/workspace.md");
const SKILL_LOADING: &str = include_str!("../instructions/skill-loading.md");
const ARTIFACTS: &str = "A board is named by its key (ROAD), its name or its id; a card by its handle (ROAD-12) or its id; a column by its name or its id; an article by its title or its id. Do not read or write anything under .cydonia/ directly — the tools keep the ids and handles straight.";

pub fn workspace() -> &'static str {
    WORKSPACE.trim()
}

pub fn skill_instructions() -> String {
    format!(
        "Built-in Cydonia skills:\n{}\n{}",
        skills::catalog(),
        SKILL_LOADING.trim()
    )
}

/// Instructions for callers with or without a bound project.
pub fn tool_context(bound: bool, skill_reader: bool) -> String {
    let project = if bound {
        "Project tools operate on the project bound to this connection."
    } else {
        "Project tools take the project's directory path. Built-in skills are independent of projects."
    };
    let mut context = format!("{}\n\n{project}\n{ARTIFACTS}", workspace());
    if skill_reader {
        context.push_str("\n\n");
        context.push_str(&skill_instructions());
    }
    context
}

/// Current session state alongside static instructions, refreshed each turn.
pub fn session_context(cwd: &Path, skill_reader: bool) -> String {
    let mut context = format!(
        "Cydonia session context\n{}\n\nCurrent project: {}\n\n",
        workspace(),
        cwd.display(),
    );
    if skill_reader {
        context.push_str(&skill_instructions());
    } else {
        context.push_str("Cydonia's MCP tools are unavailable for this session. The built-in skills below describe Cydonia's content formats; tool references do not grant access to unavailable tools. Apply the relevant instructions.\n");
        for skill in skills::list() {
            context.push_str(&format!(
                "\n--- Built-in skill: {} ---\n{}\n",
                skill.name, skill.content
            ));
        }
    }
    context
}
