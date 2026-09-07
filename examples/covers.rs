//! Cuts a cover for every article in a project that has none.
//!
//! ```sh
//! cargo run --example covers            # ../fixtures
//! cargo run --example covers -- /tmp/x
//! ```
//!
//! The content is not this program's business — articles are written by hand,
//! and this only gives them their pictures. Each one is cut through the app's
//! own [`cover::seed`] and [`cover::svg`], from the article's own path, so a
//! fixture ends up with exactly the picture cydonia would have landed on for
//! that document.
//!
//! Articles that already have a cover are left alone, so this can be run again
//! after more have been written.

use cydonia::model::cover;
use std::path::{Path, PathBuf};

const DEST: &str = "../fixtures";

fn main() {
    let dest = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEST), PathBuf::from);
    let articles = dest.join(".cydonia").join("articles");
    let Ok(entries) = std::fs::read_dir(&articles) else {
        eprintln!("no articles under {}", articles.display());
        std::process::exit(1);
    };

    let (mut cut, mut kept, mut bytes) = (0_u32, 0_u32, 0_u64);
    for entry in entries.flatten() {
        let content = entry.path().join("content.md");
        if !content.is_file() {
            continue;
        }
        // The file being there is the whole of the state — the same rule the
        // app reads covers by, so this agrees with it for free.
        if cover::of(&content).is_some() {
            kept += 1;
            continue;
        }
        match write_one(&content) {
            Some(written) => {
                bytes += written;
                cut += 1;
            }
            None => eprintln!("could not cut a cover for {}", content.display()),
        }
    }
    println!("cut {cut}, left {kept} alone — {} MB", bytes / 1_000_000);
}

fn write_one(content: &Path) -> Option<u64> {
    let seed = cover::seed(content, None);
    let path = cover::path(content, seed, "svg")?;
    let svg = cover::svg(seed);
    std::fs::write(&path, &svg).ok()?;
    Some(svg.len() as u64)
}
