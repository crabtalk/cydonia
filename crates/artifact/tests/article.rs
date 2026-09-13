//! What an article says on the wire.

mod common;

use common::Scratch;
use cydonia_artifact::{
    article::{self, Article},
    stamp,
};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};
use url::Url;

fn article(cover: Option<&str>) -> Article {
    Article {
        id: "1757000000000".into(),
        title: "Roadmap".into(),
        archived: false,
        touched: 1_757_000_000_000,
        cover: cover.map(|file| Url::from_file_path(file).expect("absolute")),
    }
}

/// A cover this backend wrote is a file, and it says so.
#[test]
fn a_cover_crosses_as_a_file_url() {
    let json = serde_json::to_value(article(Some("/tmp/x/cover-ab.svg"))).unwrap();
    assert_eq!(json["cover"], "file:///tmp/x/cover-ab.svg");
}

/// An article with no picture has no key for one, rather than a null a reader
/// has to know to expect.
#[test]
fn no_cover_is_no_key() {
    let json = serde_json::to_value(article(None)).unwrap();
    assert!(json.get("cover").is_none());
}

/// The reason this is a `Url` and not a string: a name a path has to escape
/// comes back as the same file it went in as.
#[test]
fn a_name_needing_escapes_survives_the_round_trip() {
    let file = Path::new("/tmp/a b/über/cover-01.svg");
    let out = serde_json::to_string(&article(file.to_str())).unwrap();
    let back: Article = serde_json::from_str(&out).unwrap();
    assert!(out.contains("%20"));
    assert_eq!(back.cover.unwrap().to_file_path().unwrap(), file);
}

/// Read back by a build that wrote none of these: everything optional is
/// absent, and what is left is still an article.
#[test]
fn an_older_record_reads_back() {
    let back: Article =
        serde_json::from_str(r#"{"id":"1757000000000","title":"Roadmap","touched":1757000000000}"#)
            .unwrap();
    assert_eq!(back.title, "Roadmap");
    assert!(!back.archived);
    assert!(back.cover.is_none());
}

/// Naming a page is writing it, and the name is in the properties. Ordered on
/// the markdown alone, an article that was only ever renamed sinks back down
/// the sidebar the moment it is re-read.
#[test]
fn a_page_is_touched_by_its_properties() {
    let scratch = Scratch::new("article-touched");
    let dir = article::dir(scratch.path()).join("1757000000000");
    fs::create_dir_all(&dir).unwrap();
    let content = article::content(&dir);
    fs::write(&content, "").unwrap();

    let written = article::touched(&content);
    // A second on, so the filesystem has somewhere to put it whatever its
    // stamps are rounded to.
    let later = SystemTime::now() + Duration::from_secs(1);
    let properties = fs::File::create(article::properties::path(&content).unwrap()).unwrap();
    properties.set_modified(later).unwrap();

    assert!(
        article::touched(&content) > written,
        "the properties are the later write"
    );
}

/// And an article that has none is not floated to the top by the absence —
/// `stamp::of` answers `now` for a file it cannot stat.
#[test]
fn a_page_with_no_properties_is_touched_by_its_content() {
    let scratch = Scratch::new("article-no-properties");
    let dir = article::dir(scratch.path()).join("1757000000000");
    fs::create_dir_all(&dir).unwrap();
    let content = article::content(&dir);
    fs::write(&content, "").unwrap();

    assert_eq!(article::touched(&content), stamp::of(&content));
}
