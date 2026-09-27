//! What a find bar over an article or a transcript counts as a hit: the text a
//! reader sees, not the markdown under it.

use super::*;
use artifact::session::chat::ChatItem;

#[test]
fn hits_are_found_in_rendered_text_not_markup() {
    let doc = markdown::parse("# The **pump**\n\nfeeds a pump-per-chunk `pump`");
    let query = Query::literal("PUMP").unwrap();
    let hits = hits(&doc, &query);
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].anchor.block, 0);
    assert_eq!(hits[0].anchor.offset, 4);
    assert_eq!(hits[1].anchor.block, 1);
}

#[test]
fn transcript_hits_skip_items_not_drawn_as_prose() {
    let items = vec![
        ChatItem::User("where is the pump".into()),
        ChatItem::Thinking {
            text: "pump".into(),
            done: true,
        },
        ChatItem::Agent("the *pump* runs".into()),
    ];
    let query = Query::literal("pump").unwrap();
    let hits = transcript::hits(&items, &query);
    let items: Vec<usize> = hits.iter().map(|(ix, _)| *ix).collect();
    assert_eq!(items, [0, 2]);
}

#[test]
fn current_wraps_and_is_nothing_without_hits() {
    assert_eq!(current(4, 3), Some(1));
    assert_eq!(current(0, 0), None);
}
