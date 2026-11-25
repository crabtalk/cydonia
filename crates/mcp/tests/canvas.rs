mod common;

use common::{Scratch, refused, structured};
use serde_json::{Value, json};

fn node(id: &str, x: i64) -> Value {
    json!({"id": id, "type": "text", "x": x, "y": 0, "width": 280, "height": 160,
        "text": "A card", "fontSize": 32, "textMode": "label", "plugin": {"kept": true}})
}

#[test]
fn canvas_tools_keep_stable_entries_and_refuse_stale_or_invalid_graphs() {
    let scratch = Scratch::new("canvas-tools");
    let server = scratch.server();
    let made = structured(server.call("canvas_add", json!({"title": "Map"}), Some(scratch.path())));
    let named = format!("#{}", made["number"]);
    let edited = structured(server.call(
        "canvas_edit",
        json!({
            "canvas": named, "version": made["version"], "edit": {
                "nodes": [node("a", 0), node("b", 400)],
                "edges": [{"id": "ab", "fromNode": "a", "toNode": "b", "label": "follows", "fontSize": 24}]
            }
        }),
        Some(scratch.path()),
    ));
    assert_eq!(edited["document"]["nodes"][0]["plugin"]["kept"], true);
    assert_eq!(edited["document"]["nodes"][0]["fontSize"], 32.);
    assert_eq!(edited["document"]["nodes"][0]["textMode"], "label");
    assert_eq!(edited["document"]["edges"][0]["fontSize"], 24.);
    assert_ne!(made["version"], edited["version"]);
    assert!(
        refused(server.call(
            "canvas_edit",
            json!({
                "canvas": named, "version": made["version"], "edit": {"remove_nodes": ["a"]}
            }),
            Some(scratch.path())
        ))
        .contains("changed")
    );
    assert!(
        refused(server.call(
            "canvas_edit",
            json!({
                "canvas": named, "version": edited["version"], "edit": {
                    "nodes": [node("c", 800)],
                    "edges": [{"id": "bad", "fromNode": "c", "toNode": "missing"}]
                }
            }),
            Some(scratch.path())
        ))
        .contains("existing nodes")
    );
    let read = structured(server.call(
        "canvas_read",
        json!({"canvas": named}),
        Some(scratch.path()),
    ));
    assert_eq!(read, edited);
    let removed = structured(server.call(
        "canvas_edit",
        json!({
            "canvas": named, "version": read["version"], "edit": {"remove_nodes": ["a"]}
        }),
        Some(scratch.path()),
    ));
    assert_eq!(removed["document"]["nodes"].as_array().unwrap().len(), 1);
    assert!(removed["document"].get("edges").is_none());
    let entries = structured(server.call("project_entries", json!({}), Some(scratch.path())));
    assert_eq!(entries["entries"][0]["kind"], "canvas");
    let read_entry = structured(server.call(
        "project_read_entry",
        json!({"entry": named}),
        Some(scratch.path()),
    ));
    assert_eq!(read_entry["entry"]["number"], made["number"]);
    let labelled = structured(server.call(
        "project_label_entry",
        json!({"entry": named, "labels": ["My Map"]}),
        Some(scratch.path()),
    ));
    assert_eq!(labelled["labels"], json!(["my-map"]));
}

#[test]
fn replacing_a_node_keeps_its_stacking_position_and_other_graph_fields() {
    let scratch = Scratch::new("canvas-order");
    let server = scratch.server();
    let made = structured(server.call(
        "canvas_add",
        json!({"title": "Stack"}),
        Some(scratch.path()),
    ));
    let named = format!("#{}", made["number"]);
    let added = structured(server.call("canvas_edit", json!({
        "canvas": named, "version": made["version"], "edit": {"nodes": [node("a", 0), node("b", 300)]}
    }), Some(scratch.path())));
    let moved = structured(server.call(
        "canvas_edit",
        json!({
            "canvas": named, "version": added["version"], "edit": {"nodes": [node("a", 100)]}
        }),
        Some(scratch.path()),
    ));
    assert_eq!(moved["document"]["nodes"][0]["id"], "a");
    assert_eq!(moved["document"]["nodes"][0]["x"], 100);
    assert_eq!(moved["document"]["nodes"][1], added["document"]["nodes"][1]);
}

#[test]
fn moving_or_removing_a_frame_preserves_its_comment_thread() {
    use artifact::{
        canvas::comments::Anchor,
        project::{Project as _, fs},
    };
    let scratch = Scratch::new("canvas-comments");
    let server = scratch.server();
    let made = structured(server.call(
        "canvas_add",
        json!({"title": "Design"}),
        Some(scratch.path()),
    ));
    let named = format!("#{}", made["number"]);
    let store = fs::Project::new(scratch.path());
    let mut canvas = store.canvas(made["id"].as_str().unwrap()).unwrap();
    let mut frame = node("preview", 0);
    frame["type"] = json!("web");
    frame["url"] = json!("https://example.com");
    frame.as_object_mut().unwrap().remove("text");
    canvas
        .document
        .nodes
        .push(serde_json::from_value(frame).unwrap());
    let id = canvas
        .web_comment(
            "preview".into(),
            Anchor {
                url: "https://example.com/".into(),
                selector: "#headline".into(),
                quote: "Hello".into(),
            },
            "Could this be clearer?".into(),
        )
        .unwrap();
    store.save_canvas(&mut canvas).unwrap();
    structured(server.call(
        "canvas_comment_reply",
        json!({"canvas": named, "comment": id.to_string(), "answer": "Yes, use a concrete verb."}),
        Some(scratch.path()),
    ));
    let replied = structured(server.call(
        "canvas_read",
        json!({"canvas": named}),
        Some(scratch.path()),
    ));
    let mut moved_frame = replied["document"]["nodes"][0].clone();
    moved_frame["x"] = json!(600);
    let moved = structured(server.call(
        "canvas_edit",
        json!({"canvas": named, "version": replied["version"], "edit": {"nodes": [moved_frame]}}),
        Some(scratch.path()),
    ));
    let listed = structured(server.call(
        "canvas_comments",
        json!({"canvas": named}),
        Some(scratch.path()),
    ));
    assert_eq!(listed["comments"][0]["frame_missing"], false);
    let removed = structured(server.call(
        "canvas_edit",
        json!({"canvas": named, "version": moved["version"], "edit": {"remove_nodes": ["preview"]}}),
        Some(scratch.path()),
    ));
    assert!(removed["document"]["nodes"].is_null());
    let listed = structured(server.call(
        "canvas_comments",
        json!({"canvas": named}),
        Some(scratch.path()),
    ));
    let comment = &listed["comments"][0];
    assert_eq!(comment["frame_missing"], true);
    assert_eq!(comment["anchor"]["selector"], "#headline");
    assert_eq!(
        comment["thread"]["reply"][0]["text"],
        "Yes, use a concrete verb."
    );
}
