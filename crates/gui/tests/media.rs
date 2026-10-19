//! Pictures a message carries: kept by their bytes, written as a line, read
//! back out, and scaled for the agent.

use artifact::project::{Owner, Project as _, memory};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cydonia_gui::model::{
    media::{LONG_EDGE, Picture, Shelf, attached, encode, line},
    store::Store,
};
use futures::executor::block_on;
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

/// The same bytes are filed under the one name, and other bytes under
/// another.
#[test]
fn a_picture_is_filed_under_its_bytes() {
    let store = std::sync::Arc::new(memory::Project::new());
    let shelf = Shelf {
        store: Store::Memory(store.clone()),
        owner: Owner::Shared,
    };
    let (first, write) = shelf.put(&Picture::new(b"picture".to_vec(), "PNG"));
    block_on(write).expect("filed");
    let (again, write) = shelf.put(&Picture::new(b"picture".to_vec(), "png"));
    block_on(write).expect("filed again");
    assert_eq!(first, again);
    assert!(first.starts_with("media-") && first.ends_with(".png"));
    assert_ne!(Picture::new(b"other".to_vec(), "png").name(), first);
    assert_eq!(
        block_on(store.asset(&Owner::Shared, &first)).expect("held"),
        b"picture"
    );
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
