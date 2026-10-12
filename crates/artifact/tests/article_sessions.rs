//! Sessions opened under an article: filed with every other session, named
//! by the article they are under, and carried or removed with it.

mod common;

use common::Scratch;
use cydonia_artifact::kind::Kind;
use cydonia_artifact::{
    article, entry,
    project::{Project as _, fs},
    session::record::Record,
};

fn under(store: &fs::Project, article: &str) -> String {
    let id = store.create_session().unwrap();
    let record: Record = serde_json::from_value(serde_json::json!({
        "id": id, "article": article, "agent": "claude",
        "title": "", "name": null, "updated": 1,
        "items": [{"User": "what is this?"}, {"Agent": "a word"}]
    }))
    .unwrap();
    store.save_session(&record).unwrap();
    id
}

#[test]
fn a_session_under_an_article_is_filed_and_listed_with_it() {
    let scratch = Scratch::new("article-sessions-filed");
    let store = scratch.store();
    let made = store.create_article("# Notes").unwrap();
    let id = under(&store, &made.id);

    assert!(
        store
            .cydonia()
            .join("sessions")
            .join(format!("{id}.json"))
            .is_file()
    );
    let read = store.session(&id).unwrap();
    assert_eq!(read.article.as_deref(), Some(made.id.as_str()));

    let listed = entry::catalog(&store).unwrap();
    let session = listed
        .iter()
        .find(|entry| entry.kind == Kind::Session)
        .unwrap();
    let article = listed
        .iter()
        .find(|entry| entry.kind == Kind::Article)
        .unwrap();
    assert_eq!(session.article, Some(article.number));
}

#[test]
fn an_article_moving_takes_its_sessions() {
    let (from, to) = (
        Scratch::new("article-sessions-from"),
        Scratch::new("article-sessions-to"),
    );
    let made = from.store().create_article("# Notes").unwrap();
    let id = under(&from.store(), &made.id);
    let elsewhere = from.store().create_session().unwrap();
    let other: Record = serde_json::from_value(serde_json::json!({
        "id": elsewhere, "agent": "claude", "title": "", "name": null,
        "updated": 1, "items": [{"User": "unrelated"}]
    }))
    .unwrap();
    from.store().save_session(&other).unwrap();

    let content = article::content(&article::dir(from.path()).join(&made.id));
    let arrived = article::move_to(&content, to.path()).unwrap();
    let landed = article::id_of(&arrived);

    let moved = to.store().session(&id).expect("carried to the destination");
    assert_eq!(moved.article.as_deref(), Some(landed.as_str()));
    assert_eq!(moved.items.len(), 2);
    assert!(from.store().session(&id).is_none());
    assert!(
        from.store().session(&elsewhere).is_some(),
        "a session under no article stays"
    );
}

#[test]
fn removing_an_article_removes_its_sessions() {
    let scratch = Scratch::new("article-sessions-removed");
    let store = scratch.store();
    let made = store.create_article("# Notes").unwrap();
    let id = under(&store, &made.id);
    store.remove_article(&made.id).unwrap();
    assert!(store.session(&id).is_none());
}
