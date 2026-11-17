use crate::{
    data::{ColType, Column, Data, Home},
    model::{project::Project, settings::Settings, state},
    view::{sidebar::Row, table::Cell, window::root::Cydonia},
};
use bezel::{
    gpui,
    theme::{Appearance, Theme},
};

#[gpui::test]
fn failed_table_edits_keep_the_draft_and_failed_archives_keep_the_tab(
    cx: &mut gpui::TestAppContext,
) {
    let path = std::env::temp_dir().join(format!("cydonia-table-editing-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(path.join("config")).unwrap();
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", path.join("config"));
    }
    let mut data = Data::open(&Home::Project(path.clone())).unwrap();
    let key = data
        .create(
            "Tasks",
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
    let connection = rusqlite::Connection::open(path.join(".cydonia/data.db")).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_edit BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT, 'edit refused'); END;").unwrap();
    cx.update(|cx| Theme::install(Appearance::Light, cx));
    let mut settings = Settings::default();
    settings.features.tables = true;
    let window =
        cx.add_window(|window, cx| Cydonia::new(settings, state::State::default(), window, cx));
    window
        .update(cx, |root, window, cx| {
            let member = root.workspace.update(cx, |workspace, cx| {
                let project = Project::new(workspace.intern(&path), path.clone(), cx);
                workspace.projects.push(project);
                let member = workspace.table_member(0, 0).unwrap();
                workspace.open_tab(member.clone(), crate::model::workspace::Opening::Kept, cx);
                member
            });
            let entry = member.clone().into_entry().unwrap();
            let source = root
                .workspace
                .read(cx)
                .table_entity(&entry)
                .unwrap()
                .clone();
            root.sync_leaves(window, cx);
            root.sync_items(cx);
            let view = root.table_view_of(Some(&member), cx).unwrap();
            view.update(cx, |view, cx| {
                view.cell = Some(Cell::Value {
                    rowid: 1,
                    column: "Name".into(),
                });
                view.cell_field
                    .update(cx, |field, cx| field.set_content("Keep this draft", cx));
                assert!(!view.commit_cell(cx));
                assert!(view.cell.is_some());
                assert_eq!(view.cell_field.read(cx).content(), "Keep this draft");
                assert!(
                    source
                        .read(cx)
                        .error
                        .as_ref()
                        .unwrap()
                        .contains("edit refused")
                );
            });
            root.close_project(0, cx);
            root.sync_leaves(window, cx);
            assert_eq!(root.workspace.read(cx).projects.len(), 1);
            assert!(root.table_view_of(Some(&member), cx).is_some());
            assert!(view.read(cx).cell.is_some());
            root.close_pane(&member, window, cx);
            assert!(root.table_view_of(Some(&member), cx).is_some());
            assert!(view.read(cx).cell.is_some());
            let row = Row::Entry {
                project: entry.project,
                showing: crate::model::workspace::Showing::Table(key.clone()),
            };
            root.archive_entry(&row, true, window, cx);
            assert!(!source.read(cx).record.archived);
            assert!(view.read(cx).cell.is_some());
            view.update(cx, |view, cx| {
                connection
                    .execute_batch("DROP TRIGGER reject_edit")
                    .unwrap();
                assert!(view.commit_cell(cx));
                assert!(view.cell.is_none());
            });
            assert_eq!(
                source.read(cx).page(0).unwrap().rows[0].cells[0],
                "Keep this draft"
            );
            view.update(cx, |view, cx| {
                view.cell = Some(Cell::Value {
                    rowid: 1,
                    column: "Name".into(),
                });
                view.cell_field
                    .update(cx, |field, cx| field.set_content("After schema change", cx));
            });
            connection
                .execute_batch("ALTER TABLE tasks ADD COLUMN Priority TEXT")
                .unwrap();
            root.workspace.update(cx, |workspace, cx| {
                workspace.reload_project(entry.project, cx)
            });
            view.update(cx, |view, cx| {
                assert!(view.cell.is_some());
                assert!(view.commit_cell(cx));
            });
            assert_eq!(
                source.read(cx).page(0).unwrap().rows[0].cells[0],
                "After schema change"
            );
            connection.execute_batch("CREATE TRIGGER reject_archive BEFORE UPDATE ON _tables WHEN NEW.archived = 1 BEGIN SELECT RAISE(ABORT, 'archive refused'); END;").unwrap();
            root.archive_entry(&row, true, window, cx);
            assert!(!source.read(cx).record.archived);
            assert!(
                root.workspace
                    .read(cx)
                    .strip()
                    .iter()
                    .any(|slot| slot.entry() == Some(&member))
            );
            assert!(
                source
                    .read(cx)
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("archive refused")
            );
            connection
                .execute_batch("DROP TRIGGER reject_archive")
                .unwrap();
            root.archive_entry(&row, true, window, cx);
            assert!(source.read(cx).record.archived);
            assert!(
                !root
                    .workspace
                    .read(cx)
                    .strip()
                    .iter()
                    .any(|slot| slot.entry() == Some(&member))
            );
            assert!(
                root.workspace
                    .read(cx)
                    .project(entry.project)
                    .unwrap()
                    .tables
                    .iter()
                    .all(|held| held.table.read(cx).record.archived)
            );
            root.close_project(0, cx);
            assert!(root.workspace.read(cx).projects.is_empty());
        })
        .unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
