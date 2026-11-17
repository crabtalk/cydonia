use canvas::model::Node;
use cydonia_artifact::canvas::{
    Canvas, Edit,
    web::{Frame, Source, Viewport},
};
use serde_json::json;
use std::path::{Path, PathBuf};

#[test]
fn viewport_and_source_survive_canvas_geometry_changes() {
    let mut node: Node = serde_json::from_value(json!({
        "id":"variant", "type":"web", "x":0, "y":0, "width":960, "height":600,
        "file":"designs/hero.html", "viewport":{"width":1440,"height":900}, "future":42
    }))
    .unwrap();
    let before = Frame::try_from(&node).unwrap();
    node.width = 240;
    node.height = 150;
    node.x = -120;
    assert_eq!(Frame::try_from(&node).unwrap(), before);
    let resolved = before.source.resolve(Path::new("/project")).unwrap();
    assert_eq!(resolved.url, "file:///project/designs/hero.html");
    assert_eq!(node.extra["future"], 42);
}

#[test]
fn invalid_web_source_rejects_the_whole_edit() {
    let mut canvas = Canvas::new("1".into(), "Designs", Default::default());
    let before = canvas.clone();
    let edit: Edit = serde_json::from_value(json!({"nodes":[
        {"id":"note","type":"text","x":0,"y":0,"width":100,"height":100,"text":"keep atomic"},
        {"id":"web","type":"web","x":0,"y":0,"width":100,"height":100,"url":"https://example.com","file":"page.html"}
    ]})).unwrap();
    assert!(edit.apply(&mut canvas).is_err());
    assert_eq!(canvas, before);
    assert!(Source::try_from(PathBuf::from("../page.html")).is_err());
    assert!(Source::try_from(PathBuf::from("/page.html")).is_err());
}

#[test]
fn development_server_uses_an_independent_default_viewport() {
    let mut node = Node {
        kind: "web".into(),
        url: Some("http://localhost:5173/variant".into()),
        ..Default::default()
    };
    assert_eq!(
        Frame::try_from(&node).unwrap().viewport,
        Viewport::default()
    );
    node.extra
        .insert("viewport".into(), json!({"width":0,"height":900}));
    assert!(Frame::try_from(&node).is_err());
    node.extra.clear();
    node.url = Some("javascript:alert(1)".into());
    assert!(Frame::try_from(&node).is_err());
}
