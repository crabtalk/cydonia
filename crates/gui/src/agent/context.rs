//! App context sent alongside each user prompt, including resumed sessions.

use cacp::schema::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, TextResourceContents,
};
use std::path::Path;

pub fn prompt(
    cwd: &Path,
    mcp_available: bool,
    embedded_context: bool,
    mut blocks: Vec<ContentBlock>,
) -> Vec<ContentBlock> {
    let mut context = prompts::session_context(cwd, mcp_available);
    if let Some(shown) = mcp::tools::workspace::on_screen() {
        context.push_str("\n\nOn screen in cydonia:\n");
        context.push_str(&shown);
    }
    let context = if embedded_context {
        ContentBlock::Resource(EmbeddedResource {
            resource: EmbeddedResourceResource::Text(TextResourceContents {
                uri: "cydonia://session/context".to_owned(),
                text: context,
                mime_type: Some("text/plain".to_owned()),
                meta: None,
            }),
            annotations: None,
            meta: None,
        })
    } else {
        context.into()
    };
    // Agents may derive a temporary title from the first text block.
    blocks.push(context);
    blocks
}
