//! The remote backend against a server with the `git` service in this
//! process, over Cupola's mock transport, answering as the local backend does
//! on the same repository.

extern crate cydonia_git as git;

mod fixture;

use cupola::{Client, Server, mock::Wire};
use fixture::Repo;
use git::{Backend, Git, Local, Remote};
use std::path::Path;
use tokio::runtime::Runtime;

/// A client and the server it reaches, and the wire between them.
struct Side {
    runtime: Runtime,
    remote: Remote,
    _wire: Wire,
}

impl Side {
    fn new() -> Self {
        let runtime = Runtime::new().unwrap();
        let (server, runner) = Server::new();
        Git::register(&server);
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
            runtime,
            remote: Remote::new(&client),
            _wire: wire,
        }
    }

    /// Asserts the remote backend answers every call on `dir`'s repository as
    /// the local one does, and that there is something to answer.
    fn same_as_local(&self, dir: &Path) {
        let local = self.runtime.block_on(Local.status(dir)).unwrap();
        let remote = self.runtime.block_on(self.remote.status(dir)).unwrap();
        assert_eq!(remote, local);
        let repository = local.expect("a repository");
        assert!(!repository.files.is_empty());
        for change in &repository.files {
            let root = &repository.root;
            assert_eq!(
                self.runtime
                    .block_on(self.remote.diff(root, change))
                    .unwrap(),
                self.runtime.block_on(Local.diff(root, change)).unwrap(),
            );
            assert_eq!(
                self.runtime.block_on(self.remote.sources(root, change)),
                self.runtime.block_on(Local.sources(root, change)),
            );
        }
    }
}

#[test]
fn staged_unstaged_untracked_and_renamed_answer_as_local() {
    let side = Side::new();
    let repo = Repo::new();
    repo.write("file", "head\n");
    repo.write("old name", "moved\n");
    repo.write("deleted", "gone\n");
    repo.commit();
    repo.write("file", "index\n");
    repo.git(&["add", "file"]);
    repo.write("file", "working\n");
    repo.git(&["mv", "old name", "new name"]);
    std::fs::remove_file(repo.0.join("deleted")).unwrap();
    repo.write("untracked", "new\n");
    side.same_as_local(&repo.0);
}

#[test]
fn no_repository_answers_none() {
    let side = Side::new();
    let repo = Repo::new();
    let plain = repo.0.join("plain");
    std::fs::create_dir(&plain).unwrap();
    std::fs::remove_dir_all(repo.0.join(".git")).unwrap();
    assert!(
        side.runtime
            .block_on(side.remote.status(&plain))
            .unwrap()
            .is_none()
    );
}

/// Skipped where the filesystem refuses a name that is not UTF-8.
#[cfg(unix)]
#[test]
fn non_utf8_paths_answer_as_local() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};
    let side = Side::new();
    let repo = Repo::new();
    let old = OsString::from_vec(b"old-\xff".to_vec());
    if std::fs::write(repo.0.join(&old), "text\n").is_err() {
        return;
    }
    repo.commit();
    let new = OsString::from_vec(b"new-\xff".to_vec());
    std::fs::rename(repo.0.join(&old), repo.0.join(&new)).unwrap();
    repo.git(&["add", "--all"]);
    std::fs::write(repo.0.join(&new), "changed\n").unwrap();
    std::fs::write(
        repo.0.join(OsString::from_vec(b"loose-\xfe".to_vec())),
        "x\n",
    )
    .unwrap();
    side.same_as_local(&repo.0);
}
