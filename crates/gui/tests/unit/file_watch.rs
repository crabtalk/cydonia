use crate::model::disk::{Disk, Files as _};
use std::{path::PathBuf, sync::mpsc, time::Duration};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn directory_watch_survives_atomic_save_deletion_and_recreation() {
    let dir = Directory(
        std::env::temp_dir().join(format!("cydonia-file-watch-{}", uuid::Uuid::now_v7())),
    );
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("note.md");
    std::fs::write(&path, "first").unwrap();
    let (tx, rx) = mpsc::channel();
    let _watch = Disk::watch(&path, move || {
        let _ = tx.send(());
    })
    .unwrap();
    let replacement = dir.0.join("replacement.md");
    std::fs::write(&replacement, "second").unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(Disk::read_text(&path).unwrap(), "second");
    while rx.try_recv().is_ok() {}
    std::fs::remove_file(&path).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(Disk::read_text(&path).is_err());
    while rx.try_recv().is_ok() {}
    std::fs::write(&path, "third").unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(Disk::read_text(&path).unwrap(), "third");
}
