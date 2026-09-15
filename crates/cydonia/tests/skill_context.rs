use cacp::schema::ContentBlock;
use cydonia::agent::context;
use std::path::Path;

fn text(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::Text(text) => &text.text,
        _ => panic!("expected text"),
    }
}

#[test]
fn mcp_sessions_receive_the_catalog_alongside_unchanged_user_content() {
    let blocks = context::prompt(
        Path::new("/projects/example"),
        true,
        vec!["Write an article".into()],
    );
    assert_eq!(blocks.len(), 2);
    assert!(text(&blocks[0]).contains("/projects/example"));
    assert!(text(&blocks[0]).contains(&skills::catalog()));
    assert!(text(&blocks[0]).contains("skill_read"));
    assert!(!text(&blocks[0]).contains("![Architecture overview|480]"));
    assert_eq!(text(&blocks[1]), "Write an article");
}

#[test]
fn sessions_without_mcp_receive_the_bundled_instructions_on_each_turn() {
    for _ in 0..2 {
        let blocks = context::prompt(
            Path::new("/projects/example"),
            false,
            vec!["Continue".into()],
        );
        for skill in skills::list() {
            assert!(text(&blocks[0]).contains(skill.content));
        }
        assert!(text(&blocks[0]).contains("MCP tools are unavailable"));
        assert_eq!(text(&blocks[1]), "Continue");
    }
}
