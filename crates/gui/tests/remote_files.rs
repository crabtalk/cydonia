//! `disk::Remote` against cydonia-server's services in this process, over
//! Cupola's mock transport, on a temporary directory: it lists, reads and
//! writes what `disk::Here` does there.

#![cfg(unix)]

use std::path::{Path, PathBuf};

use cupola::{Client, Server, mock::Wire};
use cydonia_gui::model::disk::{CHANGED, Files, Here, Remote};
use server::Services;
use tokio::runtime::Runtime;

struct Scratch(PathBuf);

/// A client of its own server, and the wire between them.
struct Side {
    files: Remote,
    _wire: Wire,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cydonia-remote-files-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("src/ui")).unwrap();
        std::fs::write(path.join("README.md"), "# Project\n").unwrap();
        std::fs::write(path.join("src/ui/view.rs"), "fn main() {}\n").unwrap();
        std::fs::write(path.join("src/lib.rs"), "").unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Side {
    fn open(runtime: &Runtime, root: &Path) -> Self {
        let (server, runner) = Server::new();
        Services::register(&server);
        runtime.spawn(runner);
        let (client, _) = Client::new();
        let (wire, near, far) = Wire::pair();
        let attached = client.clone();
        runtime.spawn(async move {
            let _ = attached.attach(near).await;
        });
        runtime.spawn(async move {
            let _ = server.attach(far).await;
        });
        Self {
            files: Remote::new(&client, root.to_owned()),
            _wire: wire,
        }
    }
}

fn listed(runtime: &Runtime, files: &impl Files, path: &Path) -> Vec<(bool, PathBuf)> {
    let mut listed = runtime.block_on(files.read_dir(path)).unwrap();
    listed.sort();
    listed
}

#[test]
fn listing_and_reading_match_the_local_files() {
    let scratch = Scratch::new("match");
    let runtime = Runtime::new().unwrap();
    let side = Side::open(&runtime, &scratch.0);
    let here = Here::default();
    assert!(here.local());
    assert!(!side.files.local());

    for dir in [
        scratch.0.clone(),
        scratch.0.join("src"),
        scratch.0.join("src/ui"),
    ] {
        assert_eq!(
            listed(&runtime, &side.files, &dir),
            listed(&runtime, &here, &dir),
            "{}",
            dir.display()
        );
    }
    assert_eq!(
        listed(&runtime, &side.files, Path::new("src")),
        listed(&runtime, &here, &scratch.0.join("src")),
        "a relative path is under the root"
    );

    for (file, limit) in [("README.md", 1024), ("README.md", 3), ("src/lib.rs", 8)] {
        let path = scratch.0.join(file);
        assert_eq!(
            runtime.block_on(side.files.read(&path, limit)).unwrap(),
            runtime.block_on(here.read(&path, limit)).unwrap(),
            "{file} within {limit}"
        );
    }
    let missing = scratch.0.join("missing");
    assert!(runtime.block_on(side.files.read_dir(&missing)).is_err());
    assert!(runtime.block_on(side.files.read(&missing, 8)).is_err());
}

#[test]
fn a_limited_read_holds_at_most_the_limit_and_says_it_was_cut() {
    let scratch = Scratch::new("limit");
    let runtime = Runtime::new().unwrap();
    let side = Side::open(&runtime, &scratch.0);
    let path = scratch.0.join("README.md");

    let cut = runtime.block_on(side.files.read(&path, 3)).unwrap();
    assert_eq!(cut.bytes, b"# P");
    assert!(cut.cut);
    let whole = runtime.block_on(side.files.read(&path, 10)).unwrap();
    assert_eq!(whole.bytes, b"# Project\n");
    assert!(!whole.cut);
}

#[test]
fn a_write_lands_unless_the_file_moved_on() {
    let scratch = Scratch::new("write");
    let runtime = Runtime::new().unwrap();
    let side = Side::open(&runtime, &scratch.0);
    let path = scratch.0.join("README.md");

    runtime
        .block_on(
            side.files
                .replace(&path, b"# Ours\n".to_vec(), Some(b"# Project\n".to_vec())),
        )
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"# Ours\n");

    let refused = runtime
        .block_on(
            side.files
                .replace(&path, b"# Stale\n".to_vec(), Some(b"# Project\n".to_vec())),
        )
        .unwrap_err();
    assert_eq!(refused.to_string(), CHANGED);
    assert_eq!(std::fs::read(&path).unwrap(), b"# Ours\n");

    std::fs::write(&path, b"# Theirs\n").unwrap();
    let refused = runtime
        .block_on(
            side.files
                .replace(&path, b"# Mine\n".to_vec(), Some(b"# Ours\n".to_vec())),
        )
        .unwrap_err();
    assert_eq!(refused.to_string(), CHANGED, "changed on the server's side");
    assert_eq!(std::fs::read(&path).unwrap(), b"# Theirs\n");

    let fresh = scratch.0.join("fresh.md");
    let refused = runtime
        .block_on(
            side.files
                .replace(&fresh, b"new".to_vec(), Some(Vec::new())),
        )
        .unwrap_err();
    assert_eq!(refused.to_string(), CHANGED, "a deleted file has moved on");
    assert!(!fresh.exists());

    runtime
        .block_on(side.files.replace(&path, b"# Forced\n".to_vec(), None))
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"# Forced\n");
}
