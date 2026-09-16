use super::*;

fn preview(patch: &str, old: &str, new: &str) -> Preview {
    crate::model::language::paintable();
    syntax_std::install();
    Preview::build(
        Path::new("file.rs"),
        patch.into(),
        Some(old.into()),
        Some(new.into()),
    )
}

#[test]
fn each_side_uses_its_full_file_syntax_context() {
    let preview = preview(
        "@@ -2 +2 @@\n-old words\n+    let value = \"hi\";\n",
        "/*\nold words\n*/\n",
        "fn main() {\n    let value = \"hi\";\n}\n",
    );
    let removed = &preview.lines[1];
    let added = &preview.lines[2];
    assert_eq!(removed.old, Some(2));
    assert_eq!(added.new, Some(2));
    assert!(
        removed
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::Comment)
    );
    assert!(
        added
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::Keyword)
    );
    assert!(
        added
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::String)
    );
    assert!(
        !added
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::Comment)
    );
}

#[test]
fn tabs_and_multibyte_text_keep_token_byte_ranges() {
    let source = "\tlet café = \"茶\";\n";
    let preview = preview(&format!("@@ -0,0 +1 @@\n+{source}"), "", source);
    let line = &preview.lines[1];
    assert!(line.text.starts_with("    let café"));
    assert!(
        line.spans
            .iter()
            .any(|(range, kind)| *kind == HighlightKind::String
                && line.text[range.clone()].contains('茶'))
    );
    for (range, _) in &line.spans {
        assert!(line.text.is_char_boundary(range.start));
        assert!(line.text.is_char_boundary(range.end));
    }
}

#[test]
fn hunk_counts_separate_file_headers_from_changed_code() {
    let preview = preview(
        "--- a/file.rs\n+++ b/file.rs\n@@ -7,2 +9,2 @@\n---old\n+++new\n context\n@@ -20 +30 @@\n-before\n+after\n",
        "",
        "",
    );
    assert_eq!(preview.lines[1].kind, Kind::Meta);
    assert_eq!(preview.lines[3].kind, Kind::Removed);
    assert_eq!(preview.lines[3].text, "--old");
    assert_eq!(preview.lines[4].kind, Kind::Added);
    assert_eq!(preview.lines[4].text, "++new");
    assert_eq!(
        (preview.lines[5].old, preview.lines[5].new),
        (Some(8), Some(10))
    );
    assert_eq!(
        (preview.lines[7].old, preview.lines[8].new),
        (Some(20), Some(30))
    );
}

#[test]
fn a_racing_source_read_leaves_mismatched_lines_plain() {
    let preview = preview(
        "@@ -0,0 +1 @@\n+let value = 1;\n",
        "",
        "// something else\n",
    );
    assert!(preview.lines[1].spans.is_empty());
}

#[test]
fn long_lines_are_clipped_without_cutting_utf8_or_highlights() {
    let source = format!("let text = \"{}\";\n", "茶".repeat(5000));
    let preview = preview(&format!("@@ -0,0 +1 @@\n+{source}"), "", &source);
    let line = &preview.lines[1];
    assert!(line.text.ends_with("[line truncated]"));
    for (range, _) in &line.spans {
        assert!(range.end <= line.text.len());
        assert!(line.text.is_char_boundary(range.start));
        assert!(line.text.is_char_boundary(range.end));
    }
}

#[test]
fn combined_conflicts_are_preserved_without_inventing_line_numbers() {
    let preview = preview(
        "diff --cc file.rs\n@@@ -1 -1 +1 @@@\n++<<<<<<< HEAD\n",
        "",
        "",
    );
    assert!(
        preview
            .lines
            .iter()
            .all(|line| line.kind == Kind::Meta && line.old.is_none() && line.new.is_none())
    );
    assert_eq!(preview.lines[2].text, "++<<<<<<< HEAD");
}

#[test]
fn review_hides_transport_headers_but_keeps_the_copyable_patch() {
    let patch = "diff --git a/file.rs b/file.rs\nnew file mode 100644\nindex 0000000..abcdef0\n--- /dev/null\n+++ b/file.rs\n@@ -0,0 +1,2 @@\n+fn main() {}\n+\n";
    let preview = preview(patch, "", "fn main() {}\n\n");
    let rows = preview.visible_rows(&HashSet::new());
    assert_eq!(rows.len(), 3);
    assert!(preview.lines[rows[0]].is_hunk());
    assert_eq!(preview.hunk_label(rows[0]), "Lines 1–2  ·  +2 −0");
    assert_eq!(preview.lines[rows[1]].text, "fn main() {}");
    assert_eq!(preview.lines[rows[2]].kind, Kind::Added);
    assert_eq!(preview.patch, patch);
}

#[test]
fn hunks_fold_independently_and_headers_remain_reachable() {
    let preview = preview(
        "@@ -1 +1 @@\n-before\n+after\n@@ -10 +10 @@\n-old\n+new\n",
        "",
        "",
    );
    let headers: Vec<_> = preview
        .lines
        .iter()
        .filter(|line| line.is_hunk())
        .map(|line| line.text.clone())
        .collect();
    let mut collapsed = HashSet::from([headers[0].clone()]);
    let rows = preview.visible_rows(&collapsed);
    assert_eq!(rows.len(), 4);
    assert_eq!(preview.lines[rows[0]].text, headers[0]);
    assert_eq!(preview.lines[rows[1]].text, headers[1]);
    assert!(rows.iter().all(|&ix| preview.lines[ix].text != "after"));
    collapsed.insert(headers[1].clone());
    let rows = preview.visible_rows(&collapsed);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|&ix| preview.lines[ix].is_hunk()));
    collapsed.remove(&headers[0]);
    assert!(
        preview
            .visible_rows(&collapsed)
            .iter()
            .any(|&ix| preview.lines[ix].text == "after")
    );
}

#[test]
fn metadata_only_changes_have_friendly_summaries() {
    for (patch, summary) in [
        (
            "diff --git a/empty b/empty\nnew file mode 100644\nindex 0000000..e69de29\n",
            "Empty file added",
        ),
        (
            "diff --git a/old b/new\nsimilarity index 100%\nrename from old\nrename to new\n",
            "File renamed",
        ),
        (
            "diff --git a/script b/script\nold mode 100644\nnew mode 100755\n",
            "File permissions changed",
        ),
    ] {
        let preview = Preview::plain(patch.into());
        assert!(preview.visible_rows(&HashSet::new()).is_empty());
        assert_eq!(preview.summary(), summary);
    }
}

#[test]
fn source_that_looks_like_a_git_header_is_still_visible() {
    let preview = preview(
        "@@ -0,0 +1,2 @@\n+diff --git not metadata\n+index still code\n",
        "",
        "",
    );
    assert_eq!(preview.visible_rows(&HashSet::new()).len(), 3);
}

#[test]
fn unknown_languages_keep_readable_code_and_line_numbers() {
    let preview = Preview::build(
        Path::new("file.unknown"),
        "@@ -0,0 +1 @@\n+some text\n".into(),
        None,
        Some("some text\n".into()),
    );
    assert_eq!(preview.lines[1].new, Some(1));
    assert_eq!(preview.lines[1].text, "some text");
    assert!(preview.lines[1].spans.is_empty());
}
