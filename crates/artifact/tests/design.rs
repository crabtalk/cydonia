use cydonia_artifact::design::Design;
use std::path::Path;

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
