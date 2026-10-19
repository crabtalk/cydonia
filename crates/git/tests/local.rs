//! The local backend against repositories in temporary directories.

extern crate cydonia_git as git;

mod fixture;

use fixture::Repo;
use git::{Area, Backend as _, Change, Local, Repository};
use std::path::Path;

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
    let status = Local.status(&repo.0.join("nested")).unwrap().unwrap();
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
    assert!(Local.status(&plain).unwrap().is_none());
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
    let status = Local.status(&worktree).unwrap().unwrap();
    assert_eq!(
        status.root.canonicalize().unwrap(),
        worktree.canonicalize().unwrap()
    );
    assert!(
        Local
            .diff(&status.root, &status.files[0])
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
    assert_eq!(Local.sources(&repo.0, link).new, None);
}

/// Skipped where the filesystem refuses a name that is not UTF-8.
#[cfg(unix)]
#[test]
fn non_utf8_renames_keep_both_names() {
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    };
    let repo = Repo::new();
    let old = OsString::from_vec(b"old-\xff".to_vec());
    if std::fs::write(repo.0.join(&old), "text\n").is_err() {
        return;
    }
    repo.commit();
    let new = OsString::from_vec(b"new-\xff".to_vec());
    std::fs::rename(repo.0.join(&old), repo.0.join(&new)).unwrap();
    repo.git(&["add", "--all"]);
    let files = repo.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path.as_os_str().as_bytes(), b"new-\xff");
    assert_eq!(
        files[0].original.as_ref().unwrap().as_os_str().as_bytes(),
        b"old-\xff"
    );
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
    let output = repo
        .command()
        .args(["-c", "core.hooksPath=/dev/null", "merge", "side"])
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
    let mut apply = repo
        .command()
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
fn sources_read_head_index_and_working_tree() {
    let repo = Repo::new();
    repo.write("file", "head\n");
    repo.commit();
    repo.write("file", "index\n");
    repo.git(&["add", "file"]);
    repo.write("file", "working\n");
    repo.write("untracked", "new\n");
    let files = repo.files();
    let staged = Local.sources(&repo.0, &files[0]);
    assert_eq!(staged.old.as_deref(), Some("head\n"));
    assert_eq!(staged.new.as_deref(), Some("index\n"));
    let unstaged = Local.sources(&repo.0, &files[1]);
    assert_eq!(unstaged.old.as_deref(), Some("index\n"));
    assert_eq!(unstaged.new.as_deref(), Some("working\n"));
    let untracked = Local.sources(&repo.0, &files[2]);
    assert_eq!(untracked.old, None);
    assert_eq!(untracked.new.as_deref(), Some("new\n"));
}

#[cfg(unix)]
#[test]
fn paths_cross_the_wire_as_bytes() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};
    let repository = Repository {
        root: OsString::from_vec(b"/root-\xff".to_vec()).into(),
        files: vec![Change {
            path: OsString::from_vec(b"new-\xff".to_vec()).into(),
            original: Some(OsString::from_vec(b"old-\xfe".to_vec()).into()),
            area: Area::Staged,
            status: 'R',
        }],
    };
    let json = serde_json::to_string(&repository).unwrap();
    assert_eq!(
        serde_json::from_str::<Repository>(&json).unwrap(),
        repository
    );
}
