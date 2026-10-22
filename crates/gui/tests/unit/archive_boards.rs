use super::*;
use artifact::project::fs;
#[gpui::test]
fn archived_boards_keep_their_cards(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let path =
            std::env::temp_dir().join(format!("cydonia-archive-board-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let store = fs::Project::new(&path);
        let mut board = store
            .create_board("Archive", "ARC", artifact::board::View::Lanes)
            .unwrap();
        let column = board.add_column("Todo").id.clone();
        board.add_card(&column, "Preserve this card".into());
        board.archived = true;
        store.save_board(&mut board).unwrap();
        let project = Project::new(ProjectIds::default().intern(&path), path.clone(), cx);
        assert_eq!(project.boards[0].board.read(cx).columns[0].cards.len(), 1);
        std::fs::remove_dir_all(path).unwrap();
    });
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
        article.open(cx);
        article.archive(true);
        assert!(article.document.is_some());
        article.unload(cx);
        assert!(article.document.is_none());
        assert!(article.field.is_none());
        article.open(cx);
        assert!(
            article
                .document
                .as_ref()
                .unwrap()
                .read(cx)
                .source()
                .contains("Durable article text")
        );
    });
    std::fs::remove_dir_all(path).unwrap();
}
