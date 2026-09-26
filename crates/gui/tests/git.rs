//! Git integration tests using isolated temporary repositories.
use cydonia_gui::model::git::{self, Area, Change};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
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
    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn write(&self, path: &str, text: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(path), text).unwrap();
    }
    fn commit(&self) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-m", "test: fixture"]);
    }
    fn files(&self) -> Vec<Change> {
        git::status(&self.0).unwrap().unwrap().files
    }
    fn patch(&self, file: &Change) -> String {
        git::diff(&self.0, file).unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn partially_staged_file_keeps_both_versions() {
    let repo = Repo::new();
    repo.write("file", "original\n");
    repo.commit();
    repo.write("file", "staged\n");
    repo.git(&["add", "file"]);
    repo.write("file", "working\n");
    let files = repo.files();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].area, Area::Staged);
    assert_eq!(files[1].area, Area::Unstaged);
    assert!(repo.patch(&files[0]).contains("-original\n+staged"));
    assert!(repo.patch(&files[1]).contains("-staged\n+working"));
}

#[test]
fn unborn_repo_supports_staged_and_untracked_files() {
    let repo = Repo::new();
    repo.write("staged", "first\n");
    repo.git(&["add", "staged"]);
    repo.write("untracked", "no newline");
    let files = repo.files();
    assert_eq!(files.len(), 2);
    assert!(repo.patch(&files[0]).contains("+first"));
    assert_eq!(files[1].area, Area::Untracked);
    assert!(
        repo.patch(&files[1])
            .contains("+no newline\n\\ No newline at end of file")
    );
}

#[test]
fn renames_preserve_both_paths_and_deletions_render() {
    let repo = Repo::new();
    repo.write("old name", "unchanged\n");
    repo.write("deleted", "gone\n");
    repo.commit();
    repo.git(&["mv", "old name", "new name"]);
    std::fs::remove_file(repo.0.join("deleted")).unwrap();
    let files = repo.files();
    let renamed = files.iter().find(|file| file.status == 'R').unwrap();
    assert_eq!(renamed.original.as_deref(), Some(Path::new("old name")));
    assert_eq!(renamed.path, Path::new("new name"));
    assert!(repo.patch(renamed).contains("rename to new name"));
    let deleted = files.iter().find(|file| file.status == 'D').unwrap();
    assert!(repo.patch(deleted).contains("-gone"));
}

#[test]
fn paths_are_literal_and_nested_sessions_resolve_the_root() {
    let repo = Repo::new();
    repo.write("*.txt", "literal\n");
    repo.write("other.txt", "other\n");
    repo.commit();
    repo.write("*.txt", "only this\n");
    repo.write("other.txt", "not this\n");
    std::fs::create_dir(repo.0.join("nested")).unwrap();
    let status = git::status(&repo.0.join("nested")).unwrap().unwrap();
    assert_eq!(
        status.root.canonicalize().unwrap(),
        repo.0.canonicalize().unwrap()
    );
    let file = status
        .files
        .iter()
        .find(|file| file.path == Path::new("*.txt"))
        .unwrap();
    let patch = repo.patch(file);
    assert!(patch.contains("+only this"));
    assert!(!patch.contains("not this"));
}

#[test]
fn non_repository_clean_ignored_and_binary_states() {
    let repo = Repo::new();
    repo.write(".gitignore", "ignored\n");
    repo.commit();
    repo.write("ignored", "hidden");
    assert!(repo.files().is_empty());
    repo.write("binary", [0, 1, 2]);
    assert!(repo.patch(&repo.files()[0]).contains("Binary files"));
    repo.commit();
    repo.write("binary", [0, 3, 4]);
    assert!(repo.patch(&repo.files()[0]).contains("Binary files"));
    let plain = repo.0.join("plain");
    std::fs::create_dir(&plain).unwrap();
    std::fs::remove_dir_all(repo.0.join(".git")).unwrap();
    assert!(git::status(&plain).unwrap().is_none());
}

#[test]
fn linked_worktree_is_its_own_repository() {
    let repo = Repo::new();
    repo.write("file", "base\n");
    repo.commit();
    let worktree = repo.0.join("linked");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "linked",
        worktree.to_str().unwrap(),
    ]);
    std::fs::write(worktree.join("file"), "linked change\n").unwrap();
    let status = git::status(&worktree).unwrap().unwrap();
    assert_eq!(
        status.root.canonicalize().unwrap(),
        worktree.canonicalize().unwrap()
    );
    assert!(
        git::diff(&status.root, &status.files[0])
            .unwrap()
            .contains("+linked change")
    );
}

#[test]
fn previews_are_bounded() {
    let repo = Repo::new();
    repo.write("large", "base\n");
    repo.commit();
    repo.write("large", "changed line\n".repeat(100_000));
    let patch = repo.patch(&repo.files()[0]);
    assert!(patch.contains("Preview truncated"));
    assert!(patch.len() < 1_050_000);
}

#[cfg(unix)]
#[test]
fn unusual_paths_and_symlinks_are_not_followed() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let repo = Repo::new();
    let name = OsString::from_vec(b"line\nwith-tab\t".to_vec());
    std::fs::write(repo.0.join(&name), "odd\n").unwrap();
    symlink("missing-target", repo.0.join("link")).unwrap();
    let files = repo.files();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|file| file.path.as_os_str() == name));
    let link = files
        .iter()
        .find(|file| file.path == Path::new("link"))
        .unwrap();
    assert!(repo.patch(link).contains("+missing-target"));
}

#[test]
fn merge_conflicts_appear_once_as_unmerged() {
    let repo = Repo::new();
    repo.write("file", "base\n");
    repo.commit();
    repo.git(&["checkout", "-b", "side"]);
    repo.write("file", "side\n");
    repo.commit();
    repo.git(&["checkout", "main"]);
    repo.write("file", "main\n");
    repo.commit();
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo.0)
        .args(["-c", "core.hooksPath=/dev/null", "merge", "side"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let index = std::fs::read(repo.0.join(".git/index")).unwrap();
    let files = repo.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status, 'U');
    assert_eq!(files[0].area, Area::Unstaged);
    assert!(repo.patch(&files[0]).contains("<<<<<<< HEAD"));
    assert_eq!(std::fs::read(repo.0.join(".git/index")).unwrap(), index);
}

#[test]
fn untracked_patch_is_applicable_and_empty_files_have_metadata() {
    use std::io::Write as _;
    let repo = Repo::new();
    repo.write("new file", "new text\n");
    let patch = repo.patch(&repo.files()[0]);
    std::fs::remove_file(repo.0.join("new file")).unwrap();
    let mut apply = Command::new("git")
        .arg("-C")
        .arg(&repo.0)
        .args(["apply", "--check", "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    apply
        .stdin
        .take()
        .unwrap()
        .write_all(patch.as_bytes())
        .unwrap();
    assert!(apply.wait().unwrap().success());
    repo.write("empty", "");
    let patch = repo.patch(&repo.files()[0]);
    assert!(patch.contains("new file mode"));
    assert!(!patch.contains("@@"));
}

#[test]
fn syntax_uses_head_index_and_working_versions_and_reuses_unchanged_previews() {
    cydonia_gui::model::language::paintable();
    syntax_std::install();
    use bezel::theme::HighlightKind;
    use cydonia_gui::model::git::preview::{Kind, Preview};
    use std::sync::Arc;
    let repo = Repo::new();
    repo.write("file.rs", "/*\noriginal words\n*/\n");
    repo.commit();
    repo.write("file.rs", "fn main() {\n    let staged = 1;\n}\n");
    repo.git(&["add", "file.rs"]);
    repo.write("file.rs", "fn main() {\n    let working = \"value\";\n}\n");
    let files = repo.files();
    let staged = Preview::load(&repo.0, &files[0], repo.patch(&files[0]), Arc::default());
    let removed = staged
        .lines
        .iter()
        .find(|line| line.kind == Kind::Removed && line.text == "original words")
        .unwrap();
    assert!(
        removed
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::Comment)
    );
    let unstaged = Preview::load(&repo.0, &files[1], repo.patch(&files[1]), staged);
    let added = unstaged
        .lines
        .iter()
        .find(|line| line.kind == Kind::Added)
        .unwrap();
    assert!(
        added
            .spans
            .iter()
            .any(|(_, kind)| *kind == HighlightKind::String)
    );
    let cached = Preview::load(&repo.0, &files[1], repo.patch(&files[1]), unstaged.clone());
    assert!(Arc::ptr_eq(&unstaged, &cached));
}
