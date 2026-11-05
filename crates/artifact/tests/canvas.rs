use canvas::model;
use cydonia_artifact::{
    canvas::Canvas,
    entry,
    error::Error,
    kind::Kind,
    project::{Project, fs, memory},
};
mod common;

fn lifecycle(store: &impl Project) {
    let document = model::Canvas::parse(r##"{"nodes":[{"id":"note","type":"text","x":-50,"y":90,"width":200,"height":100,"text":"hello","custom":true}],"extension":{"keep":1}}"##).unwrap();
    let mut canvas = store.create_canvas("Research", &document).unwrap();
    let number = store.number(Kind::Canvas, &canvas.id).unwrap();
    assert_eq!(
        store.resolve(Kind::Canvas, number).unwrap().as_deref(),
        Some(canvas.id.as_str())
    );
    let mut stale = store.canvas(&canvas.id).unwrap();
    canvas.title = "Ideas".into();
    canvas.archived = true;
    canvas.labels = vec!["Design work".into(), "design-work".into()];
    canvas.document.nodes[0].text = Some("updated".into());
    store.save_canvas(&mut canvas).unwrap();
    assert_eq!(canvas.labels, ["design-work"]);
    stale.title = "Old writer".into();
    assert!(matches!(store.save_canvas(&mut stale), Err(Error::Stale)));
    let loaded = store.canvas(&canvas.id).unwrap();
    assert_eq!(loaded.title, "Ideas");
    assert!(loaded.archived);
    assert_eq!(loaded.document.nodes[0].text.as_deref(), Some("updated"));
    assert_eq!(loaded.document.nodes[0].extra["custom"], true);
    assert_eq!(loaded.document.extra["extension"]["keep"], 1);
    assert_eq!(loaded.number, Some(number));
    let entries = entry::catalog(store).unwrap();
    let found = entries
        .iter()
        .find(|entry| entry.kind == Kind::Canvas)
        .unwrap();
    assert_eq!(found.title, "Ideas");
    assert_eq!(found.number, number);
    assert!(entry::open(store, found).unwrap().unwrap()["nodes"].is_array());
    store.remove_canvas(&canvas.id).unwrap();
    assert!(store.canvas(&canvas.id).is_none());
    assert!(store.resolve(Kind::Canvas, number).unwrap().is_none());
    assert!(matches!(store.save_canvas(&mut canvas), Err(Error::Stale)));
}

#[test]
fn memory_canvas_lifecycle() {
    lifecycle(&memory::Project::new());
}

#[test]
fn filesystem_canvas_lifecycle() {
    let scratch = common::Scratch::new("canvas-lifecycle");
    lifecycle(&scratch.store());
}

#[test]
fn plain_json_canvas_can_be_read_and_preserves_unknown_metadata() {
    let canvas: Canvas =
        r#"{"nodes":[],"edges":[],"unknown":"keep","cydonia":{"title":"Map","future":42}}"#
            .parse()
            .unwrap();
    assert_eq!(canvas.title, "Map");
    assert_eq!(canvas.properties.extra["future"], 42);
    assert_eq!(canvas.document.extra["unknown"], "keep");
    assert!(!canvas.document.extra.contains_key("cydonia"));
    let reopened: Canvas = canvas.to_string().parse().unwrap();
    assert_eq!(reopened.properties, canvas.properties);
    assert_eq!(reopened.document, canvas.document);
}

#[test]
fn listing_an_empty_project_does_not_create_storage() {
    let scratch = common::Scratch::new("canvas-empty");
    assert!(scratch.store().canvases().is_empty());
    assert!(!scratch.store().cydonia().exists());
}

#[test]
fn canvas_changes_are_watched() {
    let root = std::path::Path::new("/project/.cydonia");
    assert!(fs::ours(root, &root.join("canvases/1/content.canvas")));
    assert!(!fs::ours(
        root,
        &root.join("canvases/1/.content.canvas.1.part")
    ));
}

#[test]
fn seeded_canvases_use_the_same_format() {
    let store = memory::Project::seed([(
        "canvases/123/content.canvas",
        br#"{"nodes":[],"cydonia":{"title":"Seeded"}}"#.as_slice(),
    )]);
    let canvas = store.canvas("123").unwrap();
    assert_eq!(canvas.title, "Seeded");
    assert_eq!(canvas.created, Some(123));
}

#[test]
fn concurrent_canvas_creates_reserve_distinct_storage() {
    let scratch = common::Scratch::new("canvas-concurrent");
    let store = scratch.store();
    let gate = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let store = store.clone();
            let gate = gate.clone();
            std::thread::spawn(move || {
                gate.wait();
                store
                    .create_canvas(&format!("Canvas {i}"), &model::Canvas::default())
                    .unwrap()
            })
        })
        .collect();
    let mut ids: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap().id)
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 8);
    assert_eq!(store.canvases().len(), 8);
}
