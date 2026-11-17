use crate::{
    data::{ColType, Column, Data, Home},
    model::{
        project::{Project, ProjectIds},
        settings::Settings,
        state::State,
        workspace::Workspace,
    },
};
use bezel::gpui::{self, AppContext as _};
use std::path::PathBuf;

fn directory(case: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("cydonia-tables-{case}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn table(data: &mut Data, name: &str) -> String {
    let key = data
        .create(
            name,
            None,
            &[Column {
                name: "Name".into(),
                kind: ColType::Text,
            }],
            None,
        )
        .unwrap()
        .key;
    data.add_rows(&key, 1).unwrap();
    key
}

#[gpui::test]
fn table_pages_survive_selecting_another_table(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let path = directory("pages");
        let mut data = Data::open(&Home::Project(path.clone())).unwrap();
        let first = table(&mut data, "First");
        let second = table(&mut data, "Second");
        let mut project = Project::new(ProjectIds::default().intern(&path), path.clone(), cx);
        project.table = project.table_ix(&first);
        project.load_table(project.table.unwrap(), cx);
        project.table = project.table_ix(&second);
        project.load_table(project.table.unwrap(), cx);
        assert!(
            project.tables[project.table_ix(&first).unwrap()]
                .table
                .read(cx)
                .page(0)
                .is_some(),
            "the first pane still needs its page"
        );
        assert!(
            project.tables[project.table_ix(&second).unwrap()]
                .table
                .read(cx)
                .page(0)
                .is_some()
        );
        std::fs::remove_dir_all(path).unwrap();
    });
}

#[gpui::test]
fn table_archive_targets_its_project(cx: &mut gpui::TestAppContext) {
    let first = directory("first");
    let second = directory("second");
    for path in [&first, &second] {
        table(
            &mut Data::open(&Home::Project(path.clone())).unwrap(),
            "Tasks",
        );
    }
    let mut settings = Settings::default();
    settings.features.tables = true;
    let workspace = cx.new(|cx| Workspace::new(settings, State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        for path in [&first, &second] {
            let project = Project::new(workspace.intern(path), path.clone(), cx);
            workspace.projects.push(project);
        }
        let entry = workspace.table_member(1, 0).unwrap().into_entry().unwrap();
        workspace.archive_table(&entry, true, cx).unwrap();
        assert!(
            !workspace.projects[0].tables[0]
                .table
                .read(cx)
                .record
                .archived
        );
        assert!(
            workspace.projects[1].tables[0]
                .table
                .read(cx)
                .record
                .archived
        );
    });
    for path in [first, second] {
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[gpui::test]
fn editing_a_table_keeps_other_projects_and_loaded_pages_intact(cx: &mut gpui::TestAppContext) {
    let first = directory("write-first");
    let second = directory("write-second");
    for path in [&first, &second] {
        table(
            &mut Data::open(&Home::Project(path.clone())).unwrap(),
            "Tasks",
        );
    }
    let mut settings = Settings::default();
    settings.features.tables = true;
    let workspace = cx.new(|cx| Workspace::new(settings, State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        for path in [&first, &second] {
            let project = Project::new(workspace.intern(path), path.clone(), cx);
            project.load_table(0, cx);
            workspace.projects.push(project);
        }
        let entry = workspace.table_member(1, 0).unwrap().into_entry().unwrap();
        let source = workspace.table_entity(&entry).unwrap().clone();
        workspace
            .edit_table(&entry, cx, |table, cx| {
                table.write_cell(1, "Name".into(), "Second project".into(), None, cx)
            })
            .unwrap();
        assert_eq!(
            source.read(cx).page(0).unwrap().rows[0].cells[0],
            "Second project"
        );
        assert!(
            workspace.projects[0].tables[0]
                .table
                .read(cx)
                .page(0)
                .unwrap()
                .rows[0]
                .cells[0]
                .is_null()
        );
        workspace
            .rename_table(&entry, "Renamed".into(), cx)
            .unwrap();
        workspace.projects[1].reload_tables(cx);
        assert_eq!(*workspace.table_entity(&entry).unwrap(), source);
        assert_eq!(source.read(cx).page(0).unwrap().name, "Renamed");
        assert_eq!(
            workspace.projects[0].tables[0].table.read(cx).record.name,
            "Tasks"
        );
    });
    for path in [first, second] {
        std::fs::remove_dir_all(path).unwrap();
    }
}
