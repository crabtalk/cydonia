use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "cydonia-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src/ui")).unwrap();
        std::fs::write(root.join("README.md"), "# Project").unwrap();
        std::fs::write(root.join("src/ui/view.rs"), "fn main() {}").unwrap();
        Self(root)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn starts_at_root_and_only_expands_requested_folders() {
    let tree = Tree::new();
    let (rows, truncated) = scan(&tree.0, &HashSet::new(), "").unwrap();
    assert!(!truncated);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].path, tree.0.join("src"));
    assert!(rows[0].directory);
    assert_eq!(rows[1].path, tree.0.join("README.md"));
    let expanded = HashSet::from([tree.0.join("src"), tree.0.join("src/ui")]);
    let (rows, _) = scan(&tree.0, &expanded, "").unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[2].path, tree.0.join("src/ui/view.rs"));
    assert_eq!(rows[2].depth, 2);
}

#[test]
fn filtering_finds_nested_files_without_expanding_folders() {
    let tree = Tree::new();
    let (rows, _) = scan(&tree.0, &HashSet::new(), "VIEW").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, tree.0.join("src/ui/view.rs"));
    assert_eq!(rows[0].depth, 0);
}

#[cfg(unix)]
#[test]
fn folder_symlinks_do_not_recurse_outside_the_project() {
    let tree = Tree::new();
    std::os::unix::fs::symlink(&tree.0, tree.0.join("loop")).unwrap();
    let (rows, _) = scan(&tree.0, &HashSet::new(), "view").unwrap();
    assert_eq!(rows.len(), 1);
}

#[gpui::test]
fn file_tree_renders_and_reveals_selected_file(cx: &mut gpui::TestAppContext) {
    let tree = Tree::new();
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|_, cx| Files::new(tree.0.clone(), cx));
    window
        .update(cx, |files, _, cx| {
            files.reveal(Some(tree.0.join("src/ui/view.rs")), cx);
            assert!(files.expanded.contains(&tree.0.join("src")));
            assert!(files.expanded.contains(&tree.0.join("src/ui")));
        })
        .unwrap();
    let visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(220.), px(600.)));
    visual.run_until_parked();
}
