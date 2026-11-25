use crate::model::{
    project::{Project, ProjectIds},
    workspace::Entry,
};
use artifact::{
    kind::Kind,
    project::{Project as _, fs},
};
use bezel::gpui;

#[gpui::test]
fn design_watch_refreshes_only_the_touched_open_design(cx: &mut gpui::TestAppContext) {
    let root = std::env::temp_dir().join(format!("cydonia-design-watch-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let store = fs::Project::new(&root);
    let project_id = ProjectIds::default().intern(&root);
    let designs = ["First", "Second", "Closed"].map(|title| {
        let mut design = store.create_design(title).unwrap();
        let artboard = store.add_design_artboard(&mut design, title).unwrap();
        let file = store.design_dir(&design.id).unwrap().join(artboard.path());
        std::fs::write(&file, title).unwrap();
        let entry = Entry {
            project: project_id,
            kind: Kind::Design,
            id: design.id,
        };
        (entry, file)
    });
    let shown = [designs[0].0.clone(), designs[1].0.clone()];

    cx.update(|cx| {
        let mut project = Project::new(project_id, root.clone(), cx);
        let entity = |project: &Project, entry: &Entry| {
            project.designs[project.design_ix(&entry.id).unwrap()]
                .design
                .clone()
        };
        let first = entity(&project, &shown[0]);
        let second = entity(&project, &shown[1]);
        let closed = entity(&project, &designs[2].0);
        assert!(!first.read(cx).revisions_loaded);
        project.reload_designs(None, &shown, cx);
        let first_before = first.read(cx).artboards[0].revision.clone();
        let second_before = second.read(cx).artboards[0].revision.clone();
        assert!(first_before.is_some() && second_before.is_some());
        assert!(!closed.read(cx).revisions_loaded);

        for (_, file) in &designs {
            std::fs::write(file, "A rewritten artboard with a different size").unwrap();
        }
        let changed = [0, 2].map(|index| {
            designs[index]
                .1
                .strip_prefix(root.join(".cydonia"))
                .unwrap()
                .to_path_buf()
        });
        project.reload_designs(Some(&changed), &shown, cx);

        assert_ne!(first.read(cx).artboards[0].revision, first_before);
        assert_eq!(second.read(cx).artboards[0].revision, second_before);
        assert!(!closed.read(cx).revisions_loaded);
    });
    std::fs::remove_dir_all(root).unwrap();
}
