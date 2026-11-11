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
    canvas.document.nodes[0].font_size = Some(32.);
    canvas.document.nodes[0].text_mode = Some(model::TextMode::Label);
    store.save_canvas(&mut canvas).unwrap();
    assert_eq!(canvas.labels, ["design-work"]);
    stale.title = "Old writer".into();
    assert!(matches!(store.save_canvas(&mut stale), Err(Error::Stale)));
    let loaded = store.canvas(&canvas.id).unwrap();
    assert_eq!(loaded.title, "Ideas");
    assert!(loaded.archived);
    assert_eq!(loaded.document.nodes[0].text.as_deref(), Some("updated"));
    assert_eq!(loaded.document.nodes[0].text_size(), 32.);
    assert!(loaded.document.nodes[0].is_label());
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

#[test]
fn canvas_search_finds_titles_and_decoded_card_text() {
    let scratch = common::Scratch::new("canvas-search");
    let store = scratch.store();
    let document = model::Canvas::parse(r#"{"nodes":[{"id":"note","type":"text","x":0,"y":0,"width":200,"height":100,"text":"Quoted \"words\" and Unicode \u4f60\u597d"}]}"#).unwrap();
    let canvas = store.create_canvas("Research map", &document).unwrap();
    for text in ["Research", "\"words\"", "你好"] {
        let (tx, rx) = std::sync::mpsc::channel();
        let query = cydonia_artifact::search::Query::literal(text).unwrap();
        cydonia_artifact::search::disk(&store, &[Kind::Canvas], &query, &tx);
        drop(tx);
        let found: Vec<_> = rx.into_iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].item.id, canvas.id);
        assert_eq!(found[0].item.kind, Kind::Canvas);
    }
}

#[test]
fn canvas_import_export_carries_assets_and_preserves_the_graph() {
    let scratch = common::Scratch::new("canvas-bundle");
    let incoming = scratch.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::write(incoming.join("image.png"), b"image bytes").unwrap();
    let source = incoming.join("Map.canvas");
    std::fs::write(&source, r#"{"nodes":[{"id":"group","type":"group","x":-20,"y":-20,"width":500,"height":500,"background":"image.png","backgroundStyle":"repeat"},{"id":"file","type":"file","x":0,"y":0,"width":200,"height":100,"file":"image.png","custom":42}],"edges":[],"extension":{"keep":true}}"#).unwrap();
    let store = scratch.store();
    let imported = store.import_canvas(&source).unwrap();
    assert_eq!(imported.title, "Map");
    let asset = imported.document.nodes[1].file.as_ref().unwrap();
    assert!(asset.starts_with(".cydonia/canvases/"));
    assert_eq!(
        std::fs::read(scratch.path().join(asset)).unwrap(),
        b"image bytes"
    );
    assert_eq!(imported.document.nodes[0].background.as_ref(), Some(asset));
    let destination = scratch.path().join("Export.canvas");
    cydonia_artifact::canvas::bundle::Bundle::collect(imported.document.clone(), scratch.path())
        .unwrap()
        .export(&destination)
        .unwrap();
    let exported = model::Canvas::parse(&std::fs::read_to_string(&destination).unwrap()).unwrap();
    assert_eq!(exported.extra["extension"]["keep"], true);
    assert!(!exported.extra.contains_key("cydonia"));
    assert_eq!(
        exported
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["group", "file"]
    );
    assert_eq!(exported.nodes[0].x, -20);
    assert_eq!(exported.nodes[1].extra["custom"], 42);
    let exported_asset = exported.nodes[1].file.as_ref().unwrap();
    assert!(exported_asset.starts_with("Export.assets/"));
    assert_eq!(
        std::fs::read(scratch.path().join(exported_asset)).unwrap(),
        b"image bytes"
    );
    let reimported = store.import_canvas(&destination).unwrap();
    assert_eq!(
        reimported.document.nodes[0].background_style.as_deref(),
        Some("repeat")
    );
    assert_ne!(reimported.id, imported.id);
}

#[test]
fn missing_assets_do_not_create_canvas_entries_or_overwrite_exports() {
    let scratch = common::Scratch::new("canvas-missing-assets");
    let source = scratch.path().join("Missing.canvas");
    std::fs::write(&source, r#"{"nodes":[{"id":"file","type":"file","x":0,"y":0,"width":100,"height":100,"file":"gone.png"}]}"#).unwrap();
    let store = scratch.store();
    assert!(store.import_canvas(&source).is_err());
    assert!(store.canvases().is_empty());
    let document = model::Canvas::parse(&std::fs::read_to_string(&source).unwrap()).unwrap();
    assert!(cydonia_artifact::canvas::bundle::Bundle::collect(document, scratch.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(source)
            .unwrap()
            .matches("gone.png")
            .count(),
        1
    );
}
