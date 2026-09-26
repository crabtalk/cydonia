use cacp::schema::{ContentBlock, EmbeddedResourceResource};
use cydonia_gui::agent::context;
use std::path::Path;

fn text(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::Text(text) => &text.text,
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::Text(text) => &text.text,
            _ => panic!("expected text resource"),
        },
        _ => panic!("expected text"),
    }
}

#[test]
fn mcp_sessions_receive_the_catalog_alongside_unchanged_user_content() {
    let blocks = context::prompt(
        Path::new("/projects/example"),
        true,
        true,
        vec!["Write an article".into()],
    );
    assert_eq!(blocks.len(), 2);
    let ContentBlock::Resource(resource) = &blocks[1] else {
        panic!("context must be a resource");
    };
    let EmbeddedResourceResource::Text(resource) = &resource.resource else {
        panic!("context must contain text");
    };
    assert_eq!(resource.uri, "cydonia://session/context");
    assert_eq!(resource.mime_type.as_deref(), Some("text/plain"));
    assert!(text(&blocks[1]).contains("/projects/example"));
    assert!(text(&blocks[1]).contains(&prompts::resources::catalog()));
    assert!(text(&blocks[1]).contains("cydonia://resources/markdown"));
    assert!(!text(&blocks[1]).contains("![Architecture overview|480]"));
    assert_eq!(text(&blocks[0]), "Write an article");
}

#[test]
fn sessions_without_mcp_report_unavailable_resources_without_embedding_them() {
    for _ in 0..2 {
        let blocks = context::prompt(
            Path::new("/projects/example"),
            false,
            false,
            vec!["Continue".into()],
        );
        for skill in prompts::resources::list() {
            assert!(!text(&blocks[1]).contains(skill.content));
        }
        assert!(text(&blocks[1]).contains("MCP connection is unavailable"));
        assert_eq!(text(&blocks[0]), "Continue");
    }
}

#[test]
fn text_only_agents_keep_user_blocks_first() {
    let user: Vec<ContentBlock> = vec!["First part".into(), "Second part".into()];
    let blocks = context::prompt(Path::new("/project"), true, false, user.clone());
    assert_eq!(&blocks[..user.len()], user.as_slice());
    assert!(matches!(&blocks[2], ContentBlock::Text(_)));
    assert!(text(&blocks[2]).contains(&prompts::resources::catalog()));
}
