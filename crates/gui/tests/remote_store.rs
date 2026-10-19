//! `Store::Remote` against cydonia-server's services in this process, over
//! Cupola's mock transport, on a project in a temporary directory. Each
//! client has a server of its own over the one directory.

#![cfg(unix)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use artifact::{
    article::properties::Properties,
    board::Page,
    error::Error,
    kind::Kind,
    project::{Owner, Project as _, fs, remote},
    session::record::Record,
};
use cupola::{Client, Server, mock::Wire};
use cydonia_gui::{
    data::{ColType, Column, Edit},
    model::store::Store,
};
use server::Services;
use tokio::runtime::Runtime;

const PATIENCE: Duration = Duration::from_secs(10);

struct Scratch(PathBuf);

/// One client, its own server, and the wire between them.
struct Side {
    store: Store,
    _wire: Wire,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-remote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Side {
    fn open(runtime: &Runtime, root: &std::path::Path) -> Self {
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
        let (project, refresh) = runtime
            .block_on(remote::Project::open(client, root.to_owned()))
            .unwrap();
        runtime.spawn(refresh);
        Self {
            store: Store::Remote(project),
            _wire: wire,
        }
    }
}

fn runtime() -> Runtime {
    Runtime::new().unwrap()
}

fn eventually(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < PATIENCE, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn record(id: String) -> Record {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "agent": "Claude",
        "title": "Plans",
        "name": null,
        "updated": 1,
        "items": [],
    }))
    .unwrap()
}

#[test]
fn boards_articles_and_sessions_land_on_the_server() {
    let scratch = Scratch::new("round-trip");
    let runtime = runtime();
    let side = Side::open(&runtime, &scratch.0);
    let store = &side.store;
    let disk = fs::Project::new(&scratch.0);

    let made = runtime
        .block_on(store.create_board("Plans", "PLAN"))
        .unwrap();
    assert_eq!(made.number, disk.board(&made.id).unwrap().number);
    let mut board = store.board(&made.id).expect("the replica holds the board");
    assert_eq!(board.number, made.number);

    board.name = "Roadmap".into();
    board.description = "What comes next.".into();
    let column = board.add_column("Todo").id.clone();
    board.add_card(&column, "Ship it".into());
    let saved = store.save_board(&mut board);
    runtime.block_on(saved).unwrap();
    let held = disk.board(&made.id).unwrap();
    assert_eq!(held.name, "Roadmap");
    assert_eq!(held.description, "What comes next.");
    assert_eq!(
        board.version, held.version,
        "stamped as the server holds it"
    );
    assert_eq!(
        board.description_version, held.description_version,
        "stamped as the server holds it"
    );
    let card = |board: &artifact::board::Board| board.columns[0].cards[0].version.clone();
    assert_eq!(card(&board), card(&held), "stamped as the server holds it");
    assert_eq!(store.board(&made.id).unwrap().name, "Roadmap");

    board.name = "Roadmap 2".into();
    runtime.block_on(store.save_board(&mut board)).unwrap();
    assert_eq!(disk.board(&made.id).unwrap().name, "Roadmap 2");

    let article = runtime.block_on(store.create_article("# Notes\n")).unwrap();
    assert!(article.number.is_some());
    runtime
        .block_on(store.write_article(&article.id, "# Notes\n\nMore.\n"))
        .unwrap();
    assert_eq!(
        disk.read_article(&article.id).unwrap(),
        "# Notes\n\nMore.\n"
    );
    assert_eq!(
        store.read_article(&article.id).unwrap(),
        "# Notes\n\nMore.\n"
    );
    let named = Properties {
        title: "Notes".into(),
        ..Properties::default()
    };
    runtime
        .block_on(store.save_properties(&article.id, &named))
        .unwrap();
    assert_eq!(disk.properties(&article.id).title, "Notes");
    assert_eq!(store.article(&article.id).unwrap().title, "Notes");

    let id = runtime.block_on(store.create_session()).unwrap();
    runtime
        .block_on(store.save_session(&record(id.clone())))
        .unwrap();
    assert_eq!(disk.session(&id).unwrap().title, "Plans");
    let session = store.session(&id).expect("the replica holds the session");
    assert_eq!(session.number, disk.session(&id).unwrap().number);
    assert!(session.number.is_some());

    runtime.block_on(store.remove_board(&made.id)).unwrap();
    assert!(disk.board(&made.id).is_none());
    assert!(store.board(&made.id).is_none());
}

#[test]
fn an_asset_is_fetched_from_the_server() {
    let scratch = Scratch::new("asset");
    let runtime = runtime();
    let side = Side::open(&runtime, &scratch.0);
    let disk = fs::Project::new(&scratch.0);
    let article = runtime
        .block_on(side.store.create_article("# Notes\n"))
        .unwrap();
    let owner = Owner::Article(article.id.clone());
    runtime
        .block_on(disk.put_asset(&owner, "a.png", b"png"))
        .unwrap();

    assert_eq!(
        runtime.block_on(side.store.asset(&owner, "a.png")).unwrap(),
        b"png"
    );
    assert!(runtime.block_on(side.store.asset(&owner, "b.png")).is_err());
}

#[test]
fn a_cards_and_the_shared_pictures_are_written_and_fetched_on_the_server() {
    let scratch = Scratch::new("page-assets");
    let runtime = runtime();
    let side = Side::open(&runtime, &scratch.0);
    let disk = fs::Project::new(&scratch.0);
    let mut board = runtime
        .block_on(side.store.create_board("Plans", "PLAN"))
        .unwrap();
    let column = board.add_column("TODO").id.clone();
    let card = board.add_card(&column, "x".into()).unwrap().id.clone();
    runtime.block_on(side.store.save_board(&mut board)).unwrap();
    let on_card = Owner::Page {
        board: board.id.clone(),
        page: Page::Card(card.clone()),
    };
    let cards = scratch.0.join(".cydonia/boards").join(&board.id);
    let placed = [
        (
            on_card,
            cards.join("cards").join(&card).join("assets/a.png"),
        ),
        (Owner::Shared, scratch.0.join(".cydonia/assets/a.png")),
    ];
    for (owner, file) in &placed {
        runtime
            .block_on(side.store.put_asset(owner, "a.png", b"png"))
            .unwrap();
        assert_eq!(std::fs::read(file).unwrap(), b"png");
        assert_eq!(
            runtime.block_on(disk.asset(owner, "a.png")).unwrap(),
            b"png"
        );
        assert_eq!(
            runtime.block_on(side.store.asset(owner, "a.png")).unwrap(),
            b"png"
        );
    }
    let elsewhere = Owner::Page {
        board: "nope".into(),
        page: Page::Description,
    };
    assert!(
        runtime
            .block_on(side.store.put_asset(&elsewhere, "a.png", b"png"))
            .is_err()
    );
}

/// The server's root is this machine's here, so its paths are the ones the
/// disk store answers.
#[test]
fn entries_and_pictures_are_placed_at_their_paths_on_the_server() {
    let scratch = Scratch::new("places");
    let runtime = runtime();
    let side = Side::open(&runtime, &scratch.0);
    let disk = Store::Fs(fs::Project::new(&scratch.0));
    let store = &side.store;
    let mut board = runtime
        .block_on(store.create_board("Plans", "PLAN"))
        .unwrap();
    let column = board.add_column("TODO").id.clone();
    let card = board.add_card(&column, "x".into()).unwrap().id.clone();
    runtime.block_on(store.save_board(&mut board)).unwrap();
    let article = runtime.block_on(store.create_article("# Notes\n")).unwrap();
    let session = runtime.block_on(store.create_session()).unwrap();
    runtime
        .block_on(store.save_session(&record(session.clone())))
        .unwrap();

    for (kind, id) in [
        (Kind::Board, &board.id),
        (Kind::Article, &article.id),
        (Kind::Session, &session),
    ] {
        let placed = store.place(kind, id);
        assert!(placed.is_some(), "{kind:?} {id}");
        assert_eq!(placed, disk.place(kind, id), "{kind:?} {id}");
    }
    assert_eq!(store.place(Kind::Board, "nope"), None);

    let card_dir = scratch
        .0
        .join(".cydonia/boards")
        .join(&board.id)
        .join("cards")
        .join(&card);
    let article_dir = store.place(Kind::Article, &article.id).unwrap();
    for dir in [&card_dir, &article_dir] {
        assert!(store.files(dir), "{dir:?}");
    }
    assert!(!store.files(&scratch.0));

    for owner in [
        Owner::Article(article.id.clone()),
        Owner::Page {
            board: board.id.clone(),
            page: Page::Card(card.clone()),
        },
        Owner::Shared,
    ] {
        let file = store.asset_file(&owner, "a.png");
        assert!(file.is_some(), "{owner:?}");
        assert_eq!(file, disk.asset_file(&owner, "a.png"), "{owner:?}");
    }
    let elsewhere = Owner::Page {
        board: "nope".into(),
        page: Page::Description,
    };
    assert_eq!(store.asset_file(&elsewhere, "a.png"), None);
}

#[test]
fn a_table_is_made_filled_and_read_back_on_the_server() {
    let scratch = Scratch::new("tables");
    let runtime = runtime();
    let side = Side::open(&runtime, &scratch.0);
    let attached = side
        .store
        .tables()
        .expect("a remote project answers for its tables");
    assert!(runtime.block_on(attached.list()).unwrap().is_empty());
    assert!(!scratch.0.join(".cydonia").join(fs::DATA).exists());

    let mut data = side.store.make_tables().unwrap();
    let column = Column {
        name: "Name".into(),
        kind: ColType::Text,
    };
    let table = runtime
        .block_on(data.create("Notes", None, &[column], None))
        .unwrap();
    assert!(table.number.is_some());
    let rowid = runtime.block_on(data.add_rows(&table.key, 1)).unwrap()[0];
    let edit = Edit {
        rowid,
        column: "Name".into(),
        value: serde_json::json!("kept"),
    };
    // Issued before the write is answered, so read after it.
    let written = data.write_cells(&table.key, &[edit]);
    let page = data.read(&table.key, None, false, 10, 0);
    runtime.block_on(written).unwrap();
    let page = runtime.block_on(page).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].rowid, rowid);
    assert_eq!(page.rows[0].cells, [serde_json::json!("kept")]);
    assert!(scratch.0.join(".cydonia").join(fs::DATA).is_file());
}

#[test]
fn two_clients_see_the_numbers_the_server_issued() {
    let scratch = Scratch::new("numbers");
    let runtime = runtime();
    let first = Side::open(&runtime, &scratch.0);
    let board = runtime
        .block_on(first.store.create_board("Plans", "PLAN"))
        .unwrap();
    let article = runtime
        .block_on(first.store.create_article("# Notes\n"))
        .unwrap();
    let table = runtime
        .block_on(first.store.number(Kind::Table, "notes"))
        .unwrap();

    let second = Side::open(&runtime, &scratch.0);
    assert_eq!(second.store.board(&board.id).unwrap().number, board.number);
    assert_eq!(
        second.store.article(&article.id).unwrap().number,
        article.number
    );
    assert_eq!(
        second.store.resolve(Kind::Table, table).unwrap().as_deref(),
        Some("notes")
    );
    assert_eq!(
        runtime
            .block_on(second.store.number(Kind::Table, "notes"))
            .unwrap(),
        table
    );
    let numbers = [board.number.unwrap(), article.number.unwrap(), table];
    assert_eq!(
        numbers.len(),
        numbers
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        "one number per entry"
    );
}

#[test]
fn another_clients_write_knocks_the_replica_fresh() {
    let scratch = Scratch::new("knock");
    let runtime = runtime();
    let first = Side::open(&runtime, &scratch.0);
    // The directory the server watches exists before the second writes.
    let made = runtime
        .block_on(first.store.create_board("Plans", "PLAN"))
        .unwrap();
    let knocks = Arc::new(AtomicUsize::new(0));
    let counted = knocks.clone();
    let _watching = first
        .store
        .watch(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

    let second = Side::open(&runtime, &scratch.0);
    let article = runtime
        .block_on(second.store.create_article("# Theirs\n"))
        .unwrap();
    let mut board = second.store.board(&made.id).unwrap();
    board.name = "Renamed".into();
    runtime
        .block_on(second.store.save_board(&mut board))
        .unwrap();

    eventually("the first client's replica takes the article", || {
        first.store.article(&article.id).is_some()
    });
    eventually("the first client's replica takes the board", || {
        first.store.board(&made.id).unwrap().name == "Renamed"
    });
    assert_eq!(first.store.read_article(&article.id).unwrap(), "# Theirs\n");
    assert!(knocks.load(Ordering::SeqCst) > 0);
}

#[test]
fn a_stale_board_save_is_refused() {
    let scratch = Scratch::new("stale");
    let runtime = runtime();
    let first = Side::open(&runtime, &scratch.0);
    let made = runtime
        .block_on(first.store.create_board("Plans", "PLAN"))
        .unwrap();
    let second = Side::open(&runtime, &scratch.0);

    let mut ours = first.store.board(&made.id).unwrap();
    let mut theirs = second.store.board(&made.id).unwrap();
    ours.name = "Ours".into();
    runtime.block_on(first.store.save_board(&mut ours)).unwrap();

    theirs.name = "Theirs".into();
    let refused = runtime
        .block_on(second.store.save_board(&mut theirs))
        .unwrap_err();
    assert!(matches!(refused, Error::Stale), "{refused}");
    assert_eq!(
        fs::Project::new(&scratch.0).board(&made.id).unwrap().name,
        "Ours"
    );

    // The copy that saved carries the versions it wrote, so it saves again.
    ours.name = "Ours again".into();
    runtime.block_on(first.store.save_board(&mut ours)).unwrap();
}
