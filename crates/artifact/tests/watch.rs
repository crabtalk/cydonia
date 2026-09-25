//! What a project's watch lets through.

use cydonia_artifact::project::fs::ours;
use std::path::Path;

/// The filter, and the two paths that must not pass it. Written down
/// because both failures are silent: a watch that lets `-shm` through spins
/// forever against its own reads, and one that lets `sessions/` through
/// spins against its own transcripts.
#[test]
fn only_what_is_read_back_knocks() {
    let dir = Path::new("/p/.cydonia");
    for path in [
        "/p/.cydonia",
        "/p/.cydonia/articles/1/content.md",
        "/p/.cydonia/boards/1.toml",
        "/p/.cydonia/data.db",
        "/p/.cydonia/data.db-wal",
    ] {
        assert!(ours(dir, Path::new(path)), "{path} should knock");
    }
    for path in [
        "/p/.cydonia/data.db-shm",
        "/p/.cydonia/sessions/1.json",
        "/p/src/main.rs",
        "/other/.cydonia/articles/1/content.md",
    ] {
        assert!(!ours(dir, Path::new(path)), "{path} should not knock");
    }
}
