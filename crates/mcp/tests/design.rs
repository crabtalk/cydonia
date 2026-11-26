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
    assert_eq!(read["viewport"], json!({"width": 1440, "height": 900}));

    let entries = common::structured(server.call("project_entries", json!({}), Some(root)));
    assert_eq!(entries["entries"][0]["kind"], "design");
    assert_eq!(entries["entries"][0]["number"], design["number"]);
}
