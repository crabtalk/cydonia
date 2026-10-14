//! Nodes as Cydonia reads them.

use cydonia_guest::wire::{Node, Unknown};

#[test]
fn a_node_of_a_newer_contract_reads_as_its_declared_size() {
    let node: Node =
        serde_json::from_str(r#"{"node": "canvas", "width": 120, "height": 80, "ops": []}"#)
            .unwrap();
    assert_eq!(
        node,
        Node::Unknown(Unknown {
            width: Some(120.),
            height: Some(80.)
        })
    );
    let inside: Node = serde_json::from_str(
        r#"{"node": "layout", "children": [{"node": "chart"}, {"node": "text", "content": "hi"}]}"#,
    )
    .unwrap();
    let Node::Layout(layout) = inside else {
        panic!("a layout")
    };
    assert_eq!(layout.children[0], Node::Unknown(Unknown::default()));
    assert!(matches!(&layout.children[1], Node::Text(text) if text.content == "hi"));
}
