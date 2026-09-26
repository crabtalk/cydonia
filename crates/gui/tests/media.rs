//! Pictures a message carries: kept by their bytes, written as a line, read
//! back out, and scaled for the agent.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use cydonia_gui::model::media::{LONG_EDGE, attached, encode, line, store};
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cydonia-media-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A project path is free to hold a space, and the line still reads back as
/// the picture it was written for.
#[test]
fn a_line_reads_back_as_its_picture() {
    let path = PathBuf::from("/Users/me/My Project/.cydonia/assets/media-1.png");
    let text = format!("look at this\n\n{}", line(&path));
    assert_eq!(attached(&text), vec![path]);
}

/// Only local pictures go to the agent; a URL stays text.
#[test]
fn a_url_is_not_attached() {
    assert!(attached("![](https://example.com/a.png)\n\nplain words").is_empty());
}

/// The same bytes kept twice are the one file.
#[test]
fn the_same_bytes_are_one_file() {
    let dir = scratch("store");
    let first = store(&dir, b"picture", "png").expect("kept");
    let second = store(&dir, b"picture", "png").expect("kept again");
    assert_eq!(first, second);
    assert_eq!(std::fs::read_dir(&dir).expect("the directory").count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A picture past the long edge is scaled down on the way to the agent.
#[test]
fn a_large_picture_is_scaled_down() {
    let dir = scratch("encode");
    std::fs::create_dir_all(&dir).expect("the directory");
    let path = dir.join("wide.png");
    image::RgbImage::new(LONG_EDGE * 2, 100)
        .save(&path)
        .expect("written");
    let (data, mime) = encode(&path).expect("encoded");
    assert_eq!(mime, "image/png");
    let bytes = STANDARD.decode(data).expect("base64");
    let picture = image::load_from_memory(&bytes).expect("decoded");
    assert_eq!(picture.width(), LONG_EDGE);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detached_images_round_trip_without_duplication() {
    use cydonia_gui::model::media::detach;
    let source = "**Look** at this\n\n![](</tmp/first image.png>)\n\n![](</tmp/second.png>)";
    let (text, images) = detach(source);
    assert_eq!(text, "**Look** at this");
    assert_eq!(
        images,
        [
            PathBuf::from("/tmp/first image.png"),
            PathBuf::from("/tmp/second.png")
        ]
    );
    assert!(attached(&text).is_empty());
    let resent = std::iter::once(text)
        .chain(images.iter().map(|path| line(path)))
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(attached(&resent), images);

    let remote = "![](https://example.com/picture.png)";
    assert_eq!(detach(remote), (remote.to_owned(), vec![]));
    let plain = "keep  spacing\nunchanged";
    assert_eq!(detach(plain), (plain.to_owned(), vec![]));
}
