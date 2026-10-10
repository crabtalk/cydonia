use super::target;
use std::path::{Path, PathBuf};

#[test]
fn local_links_resolve_against_the_base() {
    let cwd = Path::new("/work/project");
    for (href, expected, line) in [
        (
            "src/routes/next/+page.svelte",
            "/work/project/src/routes/next/+page.svelte",
            None,
        ),
        ("./src/main.rs:42", "/work/project/src/main.rs", Some(42)),
        ("src/main.rs:42:7", "/work/project/src/main.rs", Some(42)),
        ("/other/main.rs#L12", "/other/main.rs", Some(12)),
        ("src/main.rs#L12-L20", "/work/project/src/main.rs", Some(12)),
        ("../shared/a.rs", "/work/shared/a.rs", None),
        ("file:///work/my%20file.rs#L3", "/work/my file.rs", Some(3)),
        ("docs/my%20file.md", "/work/project/docs/my file.md", None),
    ] {
        assert_eq!(
            target(Some(cwd), href),
            Some((PathBuf::from(expected), line)),
            "{href}"
        );
    }
}

#[test]
fn external_links_and_anchors_are_not_file_requests() {
    for href in [
        "https://example.com/a:80",
        "http://example.com",
        "mailto:user@example.com",
        "cydonia://session/context",
        "#heading",
        "//example.com/file",
        "",
    ] {
        assert_eq!(
            target(Some(Path::new("/work/project")), href),
            None,
            "{href}"
        );
    }
}

#[test]
fn without_a_base_only_an_absolute_path_is_a_file() {
    assert_eq!(
        target(None, "/work/main.rs:3"),
        Some((PathBuf::from("/work/main.rs"), Some(3)))
    );
    assert_eq!(
        target(None, "file:///work/main.rs"),
        Some((PathBuf::from("/work/main.rs"), None))
    );
    assert_eq!(target(None, "src/main.rs"), None);
}
