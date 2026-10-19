use artifact::{kind::Kind, location::Location};
use cydonia_gui::{
    data::{ColType, Column},
    model::{article, store},
};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-entry-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[gpui::test]
fn app_and_agent_catalog_share_article_and_table_numbers(cx: &mut gpui::TestAppContext) {
    let scratch = Scratch::new("catalog");
    let store = store::open(&Location::from(scratch.0.as_path())).expect("a store");
    let article = futures::executor::block_on(article::create(&scratch.0, store.clone())).unwrap();
    let mut data = store.make_tables().unwrap();
    let table = futures::executor::block_on(data.create(
        "Notes",
        None,
        &[Column {
            name: "text".into(),
            kind: ColType::Text,
        }],
        None,
    ))
    .unwrap();
    assert_ne!(article.number, table.number);
    futures::executor::block_on(data.write("INSERT INTO notes (text) VALUES ('hello')")).unwrap();
    let catalog = store.entries().unwrap();
    let found = catalog
        .iter()
        .find(|entry| entry.kind == Kind::Article)
        .unwrap();
    assert_eq!(Some(found.number), article.number);
    let found = catalog
        .iter()
        .find(|entry| entry.kind == Kind::Table)
        .unwrap();
    assert_eq!(Some(found.number), table.number);
    let content = store.read(found).unwrap();
    assert_eq!(content["rows"][0][0], "hello");
    cx.update(|cx| article.remove(cx));
    cx.run_until_parked();
    assert_eq!(store.entries().unwrap().len(), 1);
}
