use crate::model::{file::File, workspace::Workspace};
use bezel::gpui::{TestAppContext, WeakEntity};

#[gpui::test]
fn repeated_opens_share_the_source(cx: &mut TestAppContext) {
    let path =
        std::env::temp_dir().join(format!("cydonia-shared-source-{}.md", uuid::Uuid::now_v7()));
    let first =
        cx.update(|cx| File::open(path.clone(), WeakEntity::<Workspace>::new_invalid(), cx));
    let alias = path
        .parent()
        .unwrap()
        .join(".")
        .join(path.file_name().unwrap());
    let second = cx.update(|cx| File::open(alias, WeakEntity::new_invalid(), cx));
    assert_eq!(first.entity_id(), second.entity_id());
}

#[gpui::test]
fn local_publication_rejects_an_older_background_result(cx: &mut TestAppContext) {
    let file = cx.update(|cx| {
        File::open(
            std::env::temp_dir().join(format!(
                "cydonia-missing-source-{}.md",
                uuid::Uuid::now_v7()
            )),
            WeakEntity::new_invalid(),
            cx,
        )
    });
    file.update(cx, |file, cx| {
        let initial = file.revision;
        file.publish(Ok("saved".into()), cx);
        file.receive(initial, Ok("old read".into()), cx);
        assert_eq!(file.result, Some(Ok("saved".into())));
        file.receive(file.revision, Err("deleted".into()), cx);
        assert_eq!(file.result, Some(Err("deleted".into())));
        file.receive(file.revision, Ok("recreated".into()), cx);
        assert_eq!(file.result, Some(Ok("recreated".into())));
    });
}
