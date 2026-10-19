//! Diff previews over the local git backend, in temporary repositories.

#[path = "../../git/tests/fixture/mod.rs"]
mod fixture;

use cydonia_gui::model::language::Grammars as _;
use fixture::Repo;

#[test]
fn syntax_uses_head_index_and_working_versions_and_reuses_unchanged_previews() {
    cydonia_gui::model::language::Here::paintable();
    syntax_std::install();
    use bezel::theme::HighlightKind;
    use cydonia_gui::model::git::preview::{Kind, Preview};
    use std::sync::Arc;
    let git = git::Local;
    let repo = Repo::new();
    repo.write("file.rs", "/*\noriginal words\n*/\n");
    repo.commit();
    repo.write("file.rs", "fn main() {\n    let staged = 1;\n}\n");
    repo.git(&["add", "file.rs"]);
    repo.write("file.rs", "fn main() {\n    let working = \"value\";\n}\n");
    let files = repo.files();
    let staged = Preview::load(
        &git,
        &repo.0,
        &files[0],
        repo.patch(&files[0]),
        Arc::default(),
    );
    let removed = staged
        .lines
        .iter()
        .find(|line| line.kind == Kind::Removed && line.text == "original words")
        .unwrap();
    assert!(
        removed
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::Comment)
    );
    let unstaged = Preview::load(&git, &repo.0, &files[1], repo.patch(&files[1]), staged);
    let added = unstaged
        .lines
        .iter()
        .find(|line| line.kind == Kind::Added)
        .unwrap();
    assert!(
        added
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::String)
    );
    let cached = Preview::load(
        &git,
        &repo.0,
        &files[1],
        repo.patch(&files[1]),
        unstaged.clone(),
    );
    assert!(Arc::ptr_eq(&unstaged, &cached));
}
