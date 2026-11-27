use common::Scratch;
use cydonia_artifact::{
    design::Design,
    project::{Project, memory},
};
use std::path::Path;

mod common;

#[test]
fn legacy_pages_load_as_artboards_without_review_status() {
    let design: Design = toml::from_str(
        r#"
title = "Settings"

[[pages]]
id = "account"
title = "Account"

[pages.review]
revision = "first"
status = "approved"
"#,
    )
    .unwrap();

    assert_eq!(design.artboards[0].path(), Path::new("account/index.html"));
    let written = toml::to_string(&design).unwrap();
    assert!(written.contains("[[artboards]]"));
    assert!(!written.contains("pages"));
    assert!(!written.contains("review"));
}

#[test]
fn artboards_are_added_and_removed() {
    adds_and_removes(&Scratch::new("design-artboards").store());
    adds_and_removes(&memory::Project::default());
}

fn adds_and_removes(store: &impl Project) {
    let mut design = store.create_design("Settings").unwrap();
    let first = store.add_design_artboard(&mut design, "Account").unwrap();
    let second = store.add_design_artboard(&mut design, "Billing").unwrap();
    let titles = |design: &Design| {
        design
            .artboards
            .iter()
            .map(|artboard| artboard.title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        titles(&store.design(&design.id).unwrap()),
        ["Account", "Billing"]
    );

    store
        .remove_design_artboard(&mut design, &first.id)
        .unwrap();
    assert_eq!(design.artboards, std::slice::from_ref(&second));
    assert_eq!(titles(&store.design(&design.id).unwrap()), ["Billing"]);
    assert!(
        store
            .remove_design_artboard(&mut design, &first.id)
            .is_err()
    );
}

#[test]
fn a_removed_artboard_takes_its_folder() {
    let scratch = Scratch::new("design-artboard-folder");
    let store = scratch.store();
    let mut design = store.create_design("Settings").unwrap();
    let artboard = store.add_design_artboard(&mut design, "Account").unwrap();
    let folder = store.design_dir(&design.id).unwrap().join(&artboard.id);
    std::fs::write(folder.join("index.html"), "<p>Account</p>").unwrap();

    store
        .remove_design_artboard(&mut design, &artboard.id)
        .unwrap();
    assert!(!folder.exists());
}
