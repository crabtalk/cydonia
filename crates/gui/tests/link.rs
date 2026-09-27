#![cfg(feature = "desktop")]
//! A page's head, read into the preview a bookmark paints.

use cydonia_gui::model::link::{parse, syndication, tweet};
use url::Url;

#[test]
fn open_graph_wins_and_relative_urls_resolve() {
    let base = Url::parse("https://github.com/crabtalk/cydonia/pulls").unwrap();
    let html = r#"<!doctype html><html><HEAD>
        <title>Fallback</title>
        <meta property="og:title" content="Cydonia &amp; friends">
        <meta name=description content='A workspace — for agents'>
        <meta property="og:image" content="/card.png" />
        <link rel="shortcut icon" href="/fav.svg">
        </head><body><meta property="og:title" content="body"></body>"#;
    let preview = parse(html, &base);
    assert_eq!(preview.title.as_deref(), Some("Cydonia & friends"));
    assert_eq!(
        preview.description.as_deref(),
        Some("A workspace — for agents")
    );
    assert_eq!(
        preview.image.as_deref(),
        Some("https://github.com/card.png")
    );
    assert_eq!(preview.icon.as_deref(), Some("https://github.com/fav.svg"));
    assert_eq!(
        preview.label.as_deref(),
        Some("github.com/crabtalk/cydonia")
    );
}

#[test]
fn title_tag_and_favicon_are_the_fallbacks() {
    let base = Url::parse("https://example.com/a/b").unwrap();
    let preview = parse("<title> Plain &#233;t&eacute; </title>", &base);
    assert_eq!(preview.title.as_deref(), Some("Plain ét&eacute;"));
    assert_eq!(
        preview.icon.as_deref(),
        Some("https://example.com/favicon.ico")
    );
    assert_eq!(preview.label, None);
}

#[test]
fn an_x_post_reads_its_oembed() {
    let body = serde_json::json!({
        "author_name": "jack",
        "author_url": "https://x.com/jack",
        "html": "<blockquote class=\"twitter-tweet\"><p lang=\"en\" dir=\"ltr\">just setting up<br>my twttr &amp; more <a href=\"https://t.co/x\">pic.twitter.com/abc</a></p>&mdash; jack (@jack) <a href=\"https://x.com/jack/status/20\">March 21, 2006</a></blockquote>\n",
    });
    let preview = tweet(&body);
    assert_eq!(preview.title.as_deref(), Some("jack (@jack)"));
    assert_eq!(
        preview.description.as_deref(),
        Some("just setting up my twttr & more")
    );
    assert_eq!(preview.label.as_deref(), Some("@jack"));
}

#[test]
fn syndication_gives_the_picture_and_drops_media_links() {
    let body = serde_json::json!({
        "text": "look at this https://t.co/abc",
        "user": {
            "name": "Riley",
            "screen_name": "riley",
            "profile_image_url_https": "https://pbs.twimg.com/a_normal.jpg",
        },
        "photos": [{ "url": "https://pbs.twimg.com/media/p.jpg" }],
    });
    let preview = syndication(&body);
    assert_eq!(preview.title.as_deref(), Some("Riley (@riley)"));
    assert_eq!(preview.description.as_deref(), Some("look at this"));
    assert_eq!(
        preview.image.as_deref(),
        Some("https://pbs.twimg.com/media/p.jpg")
    );

    let body = serde_json::json!({
        "text": "words",
        "user": { "screen_name": "riley", "profile_image_url_https": "https://pbs.twimg.com/a_normal.jpg" },
    });
    assert_eq!(
        syndication(&body).image.as_deref(),
        Some("https://pbs.twimg.com/a_400x400.jpg")
    );
}
