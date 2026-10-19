//! A project's tables through the workspace: every edit is a call on the
//! store, and the list and the page are taken in when its answer lands.

use cydonia_gui::model::{
    project::Project, settings::Settings, state::State, workspace::Workspace,
};
use futures::FutureExt as _;
use gpui::AppContext as _;
use serde_json::json;
use std::path::PathBuf;

/// A scratch project, and a config directory beside it that the workspace's
/// writes land in.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("cydonia-tables-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        // Before any workspace exists, so nothing has saved yet. Sound here
        // because nextest gives each test its own process and this runs before
        // the app spawns a thread.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", root.join("config")) };
        Self(root)
    }

    fn project(&self) -> PathBuf {
        self.0.join("project")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[gpui::test]
fn a_table_made_takes_a_row_and_its_writes_in_order(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("made");
    let mut settings = Settings::default();
    settings.features.tables = true;
    let workspace = cx.new(|cx| Workspace::new(settings, State::default(), cx));
    let made = workspace.update(cx, |workspace, cx| {
        workspace
            .projects
            .push(Project::at(scratch.project().into(), cx).unwrap());
        workspace.active = Some(0);
        workspace.new_table(cx)
    });
    cx.run_until_parked();
    let (project, ix) = made.now_or_never().flatten().expect("the table landed");
    let key = workspace.read_with(cx, |workspace, _| {
        assert_eq!(workspace.projects[project].table, Some(ix));
        workspace.projects[project].tables[ix].key.clone()
    });

    let added = workspace.update(cx, |workspace, cx| workspace.add_row(&key, cx));
    cx.run_until_parked();
    let rowid = added.now_or_never().flatten().expect("the row landed");
    workspace.update(cx, |workspace, cx| {
        workspace.write_cell(&key, rowid, 0, "first".into(), cx);
        workspace.write_cell(&key, rowid, 0, "second".into(), cx);
    });
    cx.run_until_parked();

    workspace.read_with(cx, |workspace, _| {
        let page = workspace.page_at(&key).expect("the open table's page");
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].rowid, rowid);
        assert_eq!(page.rows[0].cells, [json!("second")]);
    });
}
