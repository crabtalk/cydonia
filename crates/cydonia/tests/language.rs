//! What a file's name says it is, and whether this build can paint it.

use cydonia::model::language::{Language, of, spans};
use std::path::Path;

#[test]
fn a_whole_name_beats_an_extension_and_the_longest_extension_wins() {
    assert_eq!(
        of(Path::new("Dockerfile")),
        Some(Language::Missing("dockerfile"))
    );
    assert_eq!(
        of(Path::new("a/b/Makefile")),
        Some(Language::Missing("make"))
    );
    assert_eq!(of(Path::new(".zshrc")), Some(Language::Ready("bash")));
    assert_eq!(of(Path::new("main.rs")), Some(Language::Ready("rust")));
    // `tsx` and `ts` both end the name; the longer match is the right one.
    assert_eq!(of(Path::new("App.tsx")), Some(Language::Ready("tsx")));
    assert_eq!(of(Path::new("app.ts")), Some(Language::Ready("typescript")));
}

#[test]
fn a_language_with_no_grammar_here_is_named_rather_than_unknown() {
    assert_eq!(of(Path::new("deploy.yml")), Some(Language::Missing("yaml")));
    assert_eq!(of(Path::new("index.html")), Some(Language::Missing("html")));
    assert!(spans(Path::new("deploy.yml"), "a: 1").is_none());
}

#[test]
fn a_name_the_table_does_not_carry_is_nothing_at_all() {
    assert_eq!(of(Path::new("notes.wat")), None);
    assert_eq!(of(Path::new("archive.tar.gz")), None);
    assert_eq!(of(Path::new("LICENSE")), None);
}

#[test]
fn markdown_is_painted_without_a_grammar_and_rust_with_one() {
    assert_eq!(of(Path::new("README.md")), Some(Language::Markdown));
    assert!(spans(Path::new("README.md"), "# Title\n\n**bold**\n").is_some());
    assert!(spans(Path::new("main.rs"), "fn main() {}").is_some());
}

/// Every id in the table either resolves to a grammar or is honestly missing;
/// a typo would otherwise sit there naming a language nothing can ever load.
#[test]
fn every_ready_language_is_one_syntax_actually_carries() {
    for name in [
        "main.rs", "a.py", "a.go", "a.json", "a.toml", "a.sh", "a.ts", "a.tsx",
    ] {
        assert!(
            matches!(of(Path::new(name)), Some(Language::Ready(_))),
            "{name} should be carried by this build"
        );
    }
}
