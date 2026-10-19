mod fixture;

use std::path::{Path, PathBuf};

use cydonia_cupola::{
    Client, Error, Handle,
    fs::{Entry, Expect, Fs, Hash, Kind, Request, Response},
};
use fixture::{attach, builtin_server};

async fn write(fs: &Handle<Fs>, path: &Path, bytes: &[u8], expect: Expect) -> Response {
    fs.request(&Request::Write {
        path: path.to_owned(),
        bytes: bytes.to_vec(),
        expect,
    })
    .await
    .unwrap()
}

async fn read(fs: &Handle<Fs>, path: PathBuf, limit: Option<u64>) -> Response {
    fs.request(&Request::Read { path, limit }).await.unwrap()
}

#[tokio::test]
async fn a_written_file_is_read_back_and_listed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();

    let file = dir.path().join("note.txt");
    let written = write(&fs, &file, b"hello", Expect::Anything).await;
    assert_eq!(written, Response::Written);
    assert_eq!(std::fs::read(&file).unwrap(), b"hello");

    assert_eq!(
        read(&fs, file, None).await,
        Response::Read {
            bytes: b"hello".to_vec(),
            cut: false,
        }
    );

    let listed = fs
        .request(&Request::List {
            path: dir.path().to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(
        listed,
        Response::List(vec![
            Entry {
                name: "note.txt".into(),
                kind: Kind::File,
            },
            Entry {
                name: "sub".into(),
                kind: Kind::Dir,
            },
        ])
    );
}

#[tokio::test]
async fn reading_a_missing_file_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);

    let missing = dir.path().join("missing");
    let error = client
        .service::<Fs>()
        .request(&Request::Read {
            path: missing.clone(),
            limit: None,
        })
        .await
        .unwrap_err();
    match error {
        Error::Remote { service, message } => {
            assert_eq!(service, "fs");
            assert!(message.starts_with(&format!("read {}: ", missing.display())));
        }
        other => panic!("expected a remote error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_limited_read_says_whether_it_was_cut() {
    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();
    let file = dir.path().join("note");
    std::fs::write(&file, b"hello").unwrap();

    assert_eq!(
        read(&fs, file.clone(), Some(3)).await,
        Response::Read {
            bytes: b"hel".to_vec(),
            cut: true,
        }
    );
    assert_eq!(
        read(&fs, file.clone(), Some(5)).await,
        Response::Read {
            bytes: b"hello".to_vec(),
            cut: false,
        }
    );
    assert_eq!(
        read(&fs, file, Some(0)).await,
        Response::Read {
            bytes: Vec::new(),
            cut: true,
        }
    );
}

#[tokio::test]
async fn a_write_expecting_other_content_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();
    let file = dir.path().join("note");
    std::fs::write(&file, b"theirs").unwrap();

    let refused = write(&fs, &file, b"mine", Expect::Holds(Hash::of(b"before"))).await;
    assert_eq!(refused, Response::Refused);
    assert_eq!(std::fs::read(&file).unwrap(), b"theirs");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

    let written = write(&fs, &file, b"mine", Expect::Holds(Hash::of(b"theirs"))).await;
    assert_eq!(written, Response::Written);
    assert_eq!(std::fs::read(&file).unwrap(), b"mine");

    let missing = dir.path().join("missing");
    let refused = write(&fs, &missing, b"mine", Expect::Holds(Hash::of(b""))).await;
    assert_eq!(refused, Response::Refused);
    assert!(!missing.exists());
}

#[tokio::test]
async fn a_write_expecting_no_file_is_refused_when_one_exists() {
    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();
    let file = dir.path().join("note");

    assert_eq!(
        write(&fs, &file, b"first", Expect::Absent).await,
        Response::Written
    );
    assert_eq!(
        write(&fs, &file, b"second", Expect::Absent).await,
        Response::Refused
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"first");
}

#[cfg(unix)]
#[tokio::test]
async fn a_failed_write_leaves_the_file_as_it_was() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();
    let file = dir.path().join("note");
    std::fs::write(&file, b"kept").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

    let error = fs
        .request(&Request::Write {
            path: file.clone(),
            bytes: b"lost".to_vec(),
            expect: Expect::Anything,
        })
        .await
        .unwrap_err();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(error, Error::Remote { .. }), "{error:?}");
    assert_eq!(std::fs::read(&file).unwrap(), b"kept");
}

#[cfg(unix)]
#[tokio::test]
async fn a_write_keeps_the_file_permissions_and_its_symlink() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let server = builtin_server();
    let (client, _) = Client::new();
    let _link = attach(&client, &server);
    let fs = client.service::<Fs>();
    let file = dir.path().join("script");
    std::fs::write(&file, b"old").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o751)).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&file, &link).unwrap();

    assert_eq!(
        write(&fs, &link, b"new", Expect::Anything).await,
        Response::Written
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"new");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o751);
}
