use super::typed_link;

#[test]
fn a_bare_host_is_filed_as_https() {
    assert_eq!(typed_link("example.com", None), "https://example.com");
    assert_eq!(
        typed_link("example.com/a?b#c", None),
        "https://example.com/a?b#c"
    );
}

#[test]
fn a_link_that_says_what_it_is_is_kept() {
    for typed in [
        "",
        "https://example.com",
        "mailto:me@example.com",
        "cydonia://cydonia#43",
        "#setup",
        "/etc/hosts",
        "./notes.md",
        "../notes.md",
        "~/notes.md",
        "notes",
        "two words.md",
    ] {
        assert_eq!(typed_link(typed, None), typed);
    }
}

#[test]
fn a_relative_path_to_a_file_under_the_base_is_kept() {
    let dir = std::env::temp_dir().join(format!("cydonia-ribbon-links-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.md"), "").unwrap();
    assert_eq!(typed_link("notes.md", Some(&dir)), "notes.md");
    assert_eq!(typed_link("missing.md", Some(&dir)), "https://missing.md");
    std::fs::remove_dir_all(&dir).unwrap();
}
