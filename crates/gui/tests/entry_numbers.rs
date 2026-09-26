use artifact::entry;
use cydonia_gui::{
    data::{ColType, Column, Data},
    model::article,
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

#[test]
fn table_reference_survives_key_changes_and_is_retired_on_delete() {
    let scratch = Scratch::new("table");
    let mut data = Data::open(&scratch.0).unwrap();
    let columns = [Column {
        name: "text".into(),
        kind: ColType::Text,
    }];
    let first = data.create("Notes", Some("notes"), &columns, None).unwrap();
    let reference = format!("#{}", first.number.unwrap());
    let renamed = data
        .update(&reference, Some("Final"), Some("final_notes"))
        .unwrap();
    assert_eq!(first.number, renamed.number);
    drop(data);
    let mut data = Data::open(&scratch.0).unwrap();
    assert_eq!(data.list().unwrap()[0].number, first.number);
    assert!(data.read(&reference, None, false, 200, 0).is_ok());
    data.remove(&reference).unwrap();
    let replacement = data
        .create("Final", Some("final_notes"), &columns, None)
        .unwrap();
    assert_ne!(replacement.number, first.number);
    assert!(data.read(&reference, None, false, 200, 0).is_err());
}

#[test]
fn app_and_agent_catalog_share_article_and_table_numbers() {
    let scratch = Scratch::new("catalog");
    let article = article::create(&scratch.0).unwrap();
    let mut data = Data::open(&scratch.0).unwrap();
    let table = data
        .create(
            "Notes",
            None,
            &[Column {
                name: "text".into(),
                kind: ColType::Text,
            }],
            None,
        )
        .unwrap();
    assert_ne!(article.number, table.number);
    data.write("INSERT INTO notes (text) VALUES ('hello')")
        .unwrap();
    let catalog = entry::list(&scratch.0).unwrap();
    let found = catalog
        .iter()
        .find(|entry| entry.kind == "article")
        .unwrap();
    assert_eq!(Some(found.number), article.number);
    let found = catalog.iter().find(|entry| entry.kind == "table").unwrap();
    assert_eq!(Some(found.number), table.number);
    let content = entry::read(&scratch.0, found).unwrap();
    assert_eq!(content["rows"][0][0], "hello");
    article.remove();
    assert_eq!(entry::list(&scratch.0).unwrap().len(), 1);
}
