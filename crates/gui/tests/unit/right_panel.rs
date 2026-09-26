//! The right panel across entry kinds: one per working directory, shared by
//! everything open in it.

use super::*;
use crate::model::{settings::Settings, state};
use crate::view::leaf::Pane;
use bezel::gpui;

/// A scratch project, and a config directory beside it that the test's writes
/// land in — see the same guard in `tests/unit/open_entries.rs`.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("cydonia-panel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config")).unwrap();
        unsafe { std::env::set_var("XDG_CONFIG_HOME", root.join("config")) };
        Self(root)
    }

    fn project(&self, name: &str) -> std::path::PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[gpui::test]
fn an_article_and_a_board_in_one_project_share_its_panel(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("shared");
    let path = scratch.project("one");
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(path.clone(), cx);
                workspace.new_article(cx);
                workspace.new_board(0, "First".into(), "ONE", cx).ok();
            });
            root.workspace
                .update(cx, |workspace, cx| workspace.open_article(0, 0, cx));
            root.leaf_mut().pane = Pane::Article;
            assert_eq!(root.showing(cx), Some(Pane::Article));

            // The panel opens on an article, which up to 0.1.11 it would not.
            root.show_changes(window, cx);
            assert!(root.changes_open, "the panel is up beside the article");
            let on_article = root.changes.clone().expect("a panel");
            assert_eq!(
                root.changes_for.as_deref(),
                Some(path.as_path()),
                "named by the directory, not by the entry"
            );

            // The board in the same project is the same directory, so it is
            // the same panel rather than a second one.
            root.workspace
                .update(cx, |workspace, cx| workspace.open_board(0, 0, cx));
            root.leaf_mut().pane = Pane::Board;
            assert_eq!(root.showing(cx), Some(Pane::Board));
            root.sync_changes(cx);

            assert!(root.changes_open, "and stays up across the two");
            assert_eq!(
                root.changes.as_ref(),
                Some(&on_article),
                "one panel for the directory"
            );
            assert_eq!(root.right_panels.len(), 1);
        })
        .unwrap();
}

#[gpui::test]
fn a_second_project_gets_a_panel_of_its_own(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("second");
    let (one, two) = (scratch.project("one"), scratch.project("two"));
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Light, cx);
        crate::memory::init(64 * 1024 * 1024, cx);
    });
    let window = cx.add_window(|window, cx| {
        Cydonia::new(Settings::default(), state::State::default(), window, cx)
    });

    window
        .update(cx, |root, window, cx| {
            root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(one.clone(), cx);
                workspace.new_article(cx);
            });
            root.workspace
                .update(cx, |workspace, cx| workspace.open_article(0, 0, cx));
            root.leaf_mut().pane = Pane::Article;
            root.show_changes(window, cx);
            let first = root.changes.clone().expect("a panel");

            root.workspace.update(cx, |workspace, cx| {
                workspace.open_project(two.clone(), cx);
                workspace.new_article(cx);
            });
            let project = root
                .workspace
                .read(cx)
                .projects
                .iter()
                .position(|open| open.path == two)
                .expect("the second project");
            root.workspace
                .update(cx, |workspace, cx| workspace.open_article(project, 0, cx));
            root.leaf_mut().pane = Pane::Article;
            root.sync_changes(cx);

            assert!(
                root.changes_open,
                "a directory with nothing written down opens with the panel up"
            );
            root.show_changes(window, cx);
            assert_ne!(
                root.changes.as_ref(),
                Some(&first),
                "the second project's own panel"
            );
            assert_eq!(root.right_panels.len(), 2);

            // Back to the first, which comes up as it was left.
            root.workspace
                .update(cx, |workspace, cx| workspace.open_article(0, 0, cx));
            root.leaf_mut().pane = Pane::Article;
            root.sync_changes(cx);
            assert!(root.changes_open, "left up, so it comes back up");
            assert_eq!(root.changes.as_ref(), Some(&first));
        })
        .unwrap();
}
