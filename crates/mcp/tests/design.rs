use artifact::project::{Project as _, fs};
use serde_json::json;
mod common;

#[test]
fn designs_have_stable_references_and_artboard_paths() {
    let scratch = common::Scratch::new("design-review");
    let server = scratch.server();
    let root = scratch.path();
    let design =
        common::structured(server.call("design_add", json!({"title": "Settings"}), Some(root)));
    let id = design["id"].as_str().unwrap();
    let named = format!("#{}", design["number"]);
    let artboard = common::structured(server.call(
        "design_add_artboard",
        json!({"design": named, "title": "Account"}),
        Some(root),
    ));
    let index = root.join(artboard["path"].as_str().unwrap());
    std::fs::write(
        &index,
        "<link rel=stylesheet href=style.css><h1>Account</h1>",
    )
    .unwrap();
    let held = fs::Project::new(root).design(id).unwrap();
    assert!(!held.revisions_loaded);
    let read = common::structured(server.call("design_read", json!({"design": named}), Some(root)));
    assert_eq!(read["artboards"][0]["comment_count"], 0);
    assert_eq!(read["viewport"], json!({"width": 1440, "height": 900}));

    let entries = common::structured(server.call("project_entries", json!({}), Some(root)));
    assert_eq!(entries["entries"][0]["kind"], "design");
    assert_eq!(entries["entries"][0]["number"], design["number"]);
}

#[test]
fn design_threads_survive_artboard_revisions_and_accept_replies() {
    use artifact::comment::Anchor;
    let scratch = common::Scratch::new("design-comments");
    let server = scratch.server();
    let root = scratch.path();
    let design =
        common::structured(server.call("design_add", json!({"title": "Settings"}), Some(root)));
    let named = format!("#{}", design["number"]);
    let artboard = common::structured(server.call(
        "design_add_artboard",
        json!({"design": named, "title": "Account"}),
        Some(root),
    ));
    let artboard_id = &artboard["id"];
    let index = root.join(artboard["path"].as_str().unwrap());
    std::fs::write(&index, "<h1 id=heading>Account</h1>").unwrap();
    let store = fs::Project::new(root);
    let mut held = store.design(design["id"].as_str().unwrap()).unwrap();
    held.artboards[0].comment(
        Anchor {
            url: "index.html".into(),
            selector: "#heading".into(),
            quote: "Account".into(),
        },
        "Could this be clearer?".into(),
    );
    let id = held.artboards[0].comments[0].thread.id;
    store.save_design(&mut held).unwrap();
    std::fs::write(&index, "<h1>Profile settings</h1>").unwrap();

    common::structured(server.call(
        "design_comment_reply",
        json!({
            "design": named, "artboard": artboard_id, "comment": id, "answer": "Renamed the heading."
        }),
        Some(root),
    ));
    let comments = common::structured(server.call(
        "design_comments",
        json!({"design": named, "artboard": artboard_id}),
        Some(root),
    ));
    let thread = &comments["comments"][0];
    assert_eq!(thread["anchor"]["selector"], "#heading");
    assert_eq!(thread["reply"][0]["text"], "Renamed the heading.");
}
