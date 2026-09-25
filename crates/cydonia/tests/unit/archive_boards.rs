use super::*;
use artifact::project::{Project as _, fs};
use gpui::AppContext as _;
#[test]
fn archived_boards_keep_metadata_and_load_cards_only_on_demand() {
    let path = std::env::temp_dir().join(format!("cydonia-archive-board-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let store = fs::Project::new(&path);
    let mut board = store.create_board("Archive", "ARC").unwrap();
    let column = board.add_column("Todo").id.clone();
    board.add_card(&column, "Preserve this card".into());
    board.archived = true;
    store.save_board(&mut board).unwrap();
    let mut project = Project::new(path.clone());
    assert!(project.boards[0].columns.is_empty());
    assert_eq!(project.boards[0].name, "Archive");
    assert!(project.load_board(&board.id));
    assert_eq!(project.boards[0].columns[0].cards.len(), 1);
    project.unload_boards(None);
    assert!(project.boards[0].columns.is_empty());
    assert_eq!(store.board(&board.id).unwrap().columns[0].cards.len(), 1);
    std::fs::remove_dir_all(path).unwrap();
}

#[gpui::test]
fn archived_article_releases_its_editor_and_reopens_from_disk(cx: &mut gpui::TestAppContext) {
    use crate::model::{settings::Settings, state};
    cx.update(|cx| bezel::theme::Theme::install(bezel::theme::Appearance::Light, cx));
    let path = std::env::temp_dir().join(format!("cydonia-archive-article-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |_, cx| {
        let mut article = article::create(&path).unwrap();
        std::fs::write(&article.path, "Durable article text").unwrap();
        article.open(14., cx);
        article.archive(true);
        assert!(article.editor.is_some());
        article.unload(cx);
        assert!(article.editor.is_none());
        assert!(article.field.is_none());
        article.open(14., cx);
        assert!(
            article
                .editor
                .as_ref()
                .unwrap()
                .read(cx)
                .source()
                .contains("Durable article text")
        );
    });
    std::fs::remove_dir_all(path).unwrap();
}
