//! What a file's name says it is, and whether this build can paint it.

use cydonia_gui::model::language::{Grammars as _, Here, Language, spans};
use std::path::Path;

#[test]
fn a_whole_name_beats_an_extension_and_the_longest_extension_wins() {
    assert_eq!(
        Here::of(Path::new("Dockerfile")),
        Some(Language::Missing("dockerfile"))
    );
    assert_eq!(
        Here::of(Path::new("a/b/Makefile")),
        Some(Language::Missing("make"))
    );
    assert_eq!(
        Here::of(Path::new(".zshrc")),
        Some(Language::Missing("bash"))
    );
    assert_eq!(
        Here::of(Path::new("main.rs")),
        Some(Language::Missing("rust"))
    );
    // `tsx` and `ts` both end the name; the longer match is the right one.
    assert_eq!(
        Here::of(Path::new("App.tsx")),
        Some(Language::Missing("tsx"))
    );
    assert_eq!(
        Here::of(Path::new("app.ts")),
        Some(Language::Missing("typescript"))
    );
}

#[test]
fn a_language_with_no_grammar_here_is_named_rather_than_unknown() {
    assert_eq!(
        Here::of(Path::new("deploy.yml")),
        Some(Language::Missing("yaml"))
    );
    assert_eq!(
        Here::of(Path::new("routes/+page.svelte")),
        Some(Language::Missing("svelte"))
    );
    assert_eq!(
        Here::of(Path::new("index.html")),
        Some(Language::Missing("html"))
    );
    assert!(spans(Path::new("deploy.yml"), "a: 1").is_none());
}

#[test]
fn a_name_the_table_does_not_carry_is_nothing_at_all() {
    assert_eq!(Here::of(Path::new("notes.wat")), None);
    assert_eq!(Here::of(Path::new("archive.tar.gz")), None);
    assert_eq!(Here::of(Path::new("LICENSE")), None);
}

#[test]
fn markdown_is_painted_without_installing_a_grammar() {
    assert_eq!(Here::of(Path::new("README.md")), Some(Language::Markdown));
    assert!(spans(Path::new("README.md"), "# Title\n\n**bold**\n").is_some());
    assert!(spans(Path::new("main.rs"), "fn main() {}").is_none());
}

#[test]
fn opening_a_supported_file_does_not_start_a_download() {
    use cydonia_gui::model::language::{Installable as _, Spec, Status};
    for path in [
        "main.rs", "a.py", "a.go", "a.json", "a.toml", "a.sh", "a.ts", "a.tsx", "app.js",
    ] {
        let Some(Language::Missing(name)) = Here::of(Path::new(path)) else {
            panic!("{path} should offer installation");
        };
        let spec = Spec::named(name).expect("in the catalogue");
        assert_eq!(spec.status(), Status::Missing);
    }
}
