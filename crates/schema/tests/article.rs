//! What an article says on the wire.

use cydonia_schema::article::Article;
use std::path::Path;
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
