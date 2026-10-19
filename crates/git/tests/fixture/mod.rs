//! A repository in a temporary directory, removed on drop, with system and
//! global git config ignored.

#![allow(dead_code)]

use git::{Backend as _, Change, Local};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Repo(pub PathBuf);

impl Repo {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cydonia-git-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self(path);
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Git tests"]);
        repo.git(&["config", "user.email", "git-tests@example.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }

    pub fn command(&self) -> Command {
        let mut git = Command::new("git");
        git.arg("-C")
            .arg(&self.0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        git
    }

    pub fn git(&self, args: &[&str]) {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    pub fn write(&self, path: &str, text: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(path), text).unwrap();
    }

    pub fn commit(&self) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", "test: fixture"]);
    }

    pub fn files(&self) -> Vec<Change> {
        Local.status(&self.0).unwrap().unwrap().files
    }

    pub fn patch(&self, file: &Change) -> String {
        Local.diff(&self.0, file).unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
