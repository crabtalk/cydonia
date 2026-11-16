//! App context sent alongside each user prompt, including resumed sessions.

use cydonia_acp::proto::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, TextResourceContents,
};
use std::path::Path;

pub fn prompt(
    cwd: &Path,
    session: Option<u64>,
    article: Option<u64>,
    mcp_available: bool,
    embedded_context: bool,
    mut blocks: Vec<ContentBlock>,
) -> Vec<ContentBlock> {
    let context = mcp::context::session(
        cwd,
        session,
        article,
        mcp_available,
        &super::serve::hidden_resources(),
    );
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
