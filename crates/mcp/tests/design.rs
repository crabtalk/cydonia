use artifact::{
    design::{Review, Status},
    project::{Project as _, fs},
};
use serde_json::json;
mod common;

#[test]
fn designs_have_stable_references_and_asset_edits_return_pages_to_review() {
    let scratch = common::Scratch::new("design-review");
    let server = scratch.server();
    let root = scratch.path();
    let design =
        common::structured(server.call("design_add", json!({"title": "Settings"}), Some(root)));
    let id = design["id"].as_str().unwrap();
    let named = format!("#{}", design["number"]);
    let page = common::structured(server.call(
        "design_add_page",
        json!({"design": named, "title": "Account"}),
        Some(root),
    ));
    let index = root.join(page["path"].as_str().unwrap());
    std::fs::write(
        &index,
        "<link rel=stylesheet href=style.css><h1>Account</h1>",
    )
    .unwrap();
    let style = index.with_file_name("style.css");
    std::fs::write(&style, "h1 { color: blue; }").unwrap();

    let store = fs::Project::new(root);
    let mut held = store.design(id).unwrap();
    assert_eq!(held.pages[0].status(), Status::New);
    held.pages[0].review = Some(Review {
        revision: held.pages[0].revision.clone().unwrap(),
        status: Status::Approved,
    });
    store.save_design(&mut held).unwrap();

    let read =
        || common::structured(server.call("design_read", json!({"design": named}), Some(root)));
    assert_eq!(read()["pages"][0]["status"], "approved");
    assert_eq!(read()["pages"][0]["comment_count"], 0);
    std::fs::write(&style, "h1 { color: rebeccapurple; }").unwrap();
    assert_eq!(read()["pages"][0]["status"], "new");

    let entries = common::structured(server.call("project_entries", json!({}), Some(root)));
    assert_eq!(entries["entries"][0]["kind"], "design");
    assert_eq!(entries["entries"][0]["number"], design["number"]);
}
