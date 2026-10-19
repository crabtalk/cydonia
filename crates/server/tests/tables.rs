//! The `tables` service, over [`common::Served`].

mod common;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use artifact::{
    entry,
    kind::Kind,
    project::fs::DATA,
    table::{ColType, Column, Tables as _, remote::Tables},
};
use common::{Scratch, Served};
use cupola::ping::Ping;
use serde_json::{Map, Value, json};

const PROMPT: Duration = Duration::from_secs(1);

/// [`Served`], with the tables of the project at one root.
struct Side {
    served: Served,
    tables: Tables,
}

impl Side {
    fn new(root: &Path) -> Self {
        let served = Served::new();
        let tables = Tables::new(&served.client, root.to_owned());
        Self { served, tables }
    }
}

fn row(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(column, value)| ((*column).to_owned(), value.clone()))
        .collect()
}

#[test]
fn browsing_makes_no_store() {
    let scratch = Scratch::new("browse");
    let side = Side::new(scratch.path());
    let listed = side.served.runtime.block_on(side.tables.list()).unwrap();
    assert!(listed.is_empty());
    assert!(
        side.served
            .runtime
            .block_on(side.tables.query("SELECT 1"))
            .is_err()
    );
    assert!(!scratch.path().join(".cydonia").join(DATA).exists());
}

#[test]
fn rows_written_read_back() {
    let scratch = Scratch::new("rows");
    let mut side = Side::new(scratch.path());
    let columns = [
        Column {
            name: "title".into(),
            kind: ColType::Text,
        },
        Column {
            name: "pages".into(),
            kind: ColType::Number,
        },
    ];
    let made = side
        .served
        .runtime
        .block_on(side.tables.create("Books", None, &columns, Some("agent")))
        .unwrap();
    assert_eq!(made.key, "books");
    assert_eq!(made.author.as_deref(), Some("agent"));
    let number = made.number.unwrap();
    let registry = entry::Registry::open(scratch.path()).unwrap();
    assert_eq!(
        registry.resolve(Kind::Table, number).unwrap().as_deref(),
        Some("books")
    );

    let ids = side
        .served
        .runtime
        .block_on(side.tables.insert(
            "books",
            &[
                row(&[("title", json!("Dune")), ("pages", json!(412))]),
                row(&[("title", json!("Solaris")), ("pages", json!(204))]),
            ],
        ))
        .unwrap();
    assert_eq!(ids.len(), 2);

    let page = side
        .served
        .runtime
        .block_on(side.tables.read("Books", Some("pages"), false, 10, 0))
        .unwrap();
    assert_eq!(page.total, 2);
    let titles: Vec<&Value> = page.rows.iter().map(|row| &row.cells[0]).collect();
    assert_eq!(titles, [&json!("Solaris"), &json!("Dune")]);

    let rows = side
        .served
        .runtime
        .block_on(
            side.tables
                .query("SELECT title FROM books WHERE pages > 300"),
        )
        .unwrap();
    assert_eq!(rows.columns, ["title"]);
    assert_eq!(rows.rows, [vec![json!("Dune")]]);

    let listed = side.served.runtime.block_on(side.tables.list()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].number, made.number);
    assert_eq!(listed[0].rows, 2);

    let reference = format!("#{number}");
    let renamed = side
        .served
        .runtime
        .block_on(side.tables.update(&reference, Some("Novels"), None))
        .unwrap();
    assert_eq!(renamed.name, "Novels");

    let missing = side
        .served
        .runtime
        .block_on(side.tables.read("nothing", None, false, 10, 0))
        .unwrap_err();
    assert_eq!(missing.to_string(), "no table nothing");
}

#[test]
fn writes_land_in_the_order_they_were_sent() {
    const SENT: i64 = 40;
    let scratch = Scratch::new("order");
    let mut side = Side::new(scratch.path());
    let columns = [Column {
        name: "n".into(),
        kind: ColType::Number,
    }];
    side.served
        .runtime
        .block_on(side.tables.create("Seq", None, &columns, None))
        .unwrap();

    let inserts: Vec<_> = (0..SENT)
        .map(|n| side.tables.insert("seq", &[row(&[("n", json!(n))])]))
        .collect();
    for inserted in side
        .served
        .runtime
        .block_on(futures::future::join_all(inserts))
    {
        inserted.unwrap();
    }
    let rows = side
        .served
        .runtime
        .block_on(side.tables.query("SELECT n FROM seq ORDER BY rowid"))
        .unwrap();
    let held: Vec<Value> = rows.rows.into_iter().flatten().collect();
    let sent: Vec<Value> = (0..SENT).map(|n| json!(n)).collect();
    assert_eq!(held, sent);

    let updates: Vec<_> = (0..SENT)
        .map(|n| side.tables.write(&format!("UPDATE seq SET n = {n}")))
        .collect();
    for updated in side
        .served
        .runtime
        .block_on(futures::future::join_all(updates))
    {
        updated.unwrap();
    }
    let rows = side
        .served
        .runtime
        .block_on(side.tables.query("SELECT DISTINCT n FROM seq"))
        .unwrap();
    assert_eq!(rows.rows, [vec![json!(SENT - 1)]]);
}

#[test]
fn a_held_root_holds_neither_a_ping_nor_another_root() {
    let held = Scratch::new("held");
    let other = Scratch::new("other");
    let mut side = Side::new(held.path());
    let columns = [Column {
        name: "n".into(),
        kind: ColType::Number,
    }];
    side.served
        .runtime
        .block_on(side.tables.create("Seq", None, &columns, None))
        .unwrap();

    let lock = rusqlite::Connection::open(held.path().join(".cydonia").join(DATA)).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let waiting = side.tables.insert("seq", &[row(&[("n", json!(1))])]);
    std::thread::sleep(PROMPT / 4);

    let asked = Instant::now();
    side.served
        .runtime
        .block_on(side.served.client.service::<Ping>().send(&()))
        .unwrap();
    let elsewhere = Tables::new(&side.served.client, other.path().to_owned());
    let listed = side.served.runtime.block_on(elsewhere.list()).unwrap();
    assert!(listed.is_empty());
    assert!(asked.elapsed() < PROMPT);

    lock.execute_batch("COMMIT").unwrap();
    assert_eq!(side.served.runtime.block_on(waiting).unwrap().len(), 1);
}
