//! App context sent alongside each user prompt, including resumed sessions.

use cacp::schema::ContentBlock;
use std::path::Path;

pub fn prompt(cwd: &Path, mcp_available: bool, blocks: Vec<ContentBlock>) -> Vec<ContentBlock> {
    let mut context = format!(
        "Cydonia session context\nYou are working inside Cydonia.\nCurrent project: {}\n\n",
        cwd.display(),
    );
    if mcp_available {
        context.push_str(&::mcp::tools::skill::instructions());
    } else {
        context.push_str("Cydonia's MCP tools are unavailable for this session. The built-in skills below describe Cydonia's content formats; tool references do not grant access to unavailable tools. Apply the relevant instructions.\n");
        for skill in skills::list() {
            context.push_str(&format!(
                "\n--- Built-in skill: {} ---\n{}\n",
                skill.name, skill.content
            ));
        }
    }
    std::iter::once(context.into()).chain(blocks).collect()
}
