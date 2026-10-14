//! An app on the runner: `cydonia-guest`'s counter example, built for
//! `wasm32-unknown-unknown` and driven through `Runner`.

use cydonia_gui::model::apps::{Command, Event, Node, Runner as _, Runtime};
use guest::wire;
use std::{path::PathBuf, process::Command as Process, sync::OnceLock};

/// The counter example's `.wasm`, built once per test run.
fn counter() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("guest");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Process::new(cargo)
            .current_dir(&root)
            .args([
                "build",
                "-p",
                "cydonia-guest",
                "--example",
                "counter",
                "--target",
                "wasm32-unknown-unknown",
                "--target-dir",
            ])
            .arg(&target)
            .status()
            .expect("cargo runs");
        assert!(status.success(), "the counter example builds for wasm32");
        std::fs::read(target.join("wasm32-unknown-unknown/debug/examples/counter.wasm"))
            .expect("the counter example's wasm")
    })
}

fn texts(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Layout(layout) => layout.children.iter().for_each(|child| texts(child, out)),
        Node::Text(text) => out.push(text.content.clone()),
        _ => {}
    }
}

fn shows(node: &Node, text: &str) -> bool {
    let mut out = Vec::new();
    texts(node, &mut out);
    out.iter().any(|shown| shown == text)
}

fn find<'a, T>(node: &'a Node, pick: &impl Fn(&'a Node) -> Option<T>) -> Option<T> {
    if let Some(found) = pick(node) {
        return Some(found);
    }
    match node {
        Node::Layout(layout) => layout.children.iter().find_map(|child| find(child, pick)),
        _ => None,
    }
}

fn button(node: &Node, label: &str) -> wire::Handler {
    find(node, &|node| match node {
        Node::Button(button) if button.label == label => button.on_click,
        _ => None,
    })
    .unwrap_or_else(|| panic!("a {label} button"))
}

#[test]
fn init_sets_an_input_and_a_click_redraws() {
    let (mut app, commands) = Runtime::start(counter()).unwrap();
    assert_eq!(
        commands,
        [Command::SetInput {
            key: "name".into(),
            value: "world".into()
        }]
    );
    let view = app.view().unwrap();
    assert!(shows(&view, "Hello, world"));
    assert!(shows(&view, "0"));

    let add = button(&view, "Add");
    assert!(
        app.update(&Event::Click { handler: add })
            .unwrap()
            .is_empty()
    );
    assert!(shows(&app.view().unwrap(), "1"));
}

#[test]
fn input_and_select_send_their_text() {
    let (mut app, _) = Runtime::start(counter()).unwrap();
    let view = app.view().unwrap();
    let on_input = find(&view, &|node| match node {
        Node::Input(input) => input.on_input,
        _ => None,
    })
    .unwrap();
    app.update(&Event::Input {
        handler: on_input,
        text: "Ada".into(),
    })
    .unwrap();
    let view = app.view().unwrap();
    assert!(shows(&view, "Hello, Ada"));

    let (on_select, selected) = find(&view, &|node| match node {
        Node::Select(select) => Some((select.on_select?, select.selected.clone())),
        _ => None,
    })
    .unwrap();
    assert_eq!(selected, None);
    app.update(&Event::Select {
        handler: on_select,
        value: "green".into(),
    })
    .unwrap();
    let selected = find(&app.view().unwrap(), &|node| match node {
        Node::Select(select) => Some(select.selected.clone()),
        _ => None,
    })
    .unwrap();
    assert_eq!(selected.as_deref(), Some("green"));
}

#[test]
fn reset_clears_the_input_through_cydonia() {
    let (mut app, _) = Runtime::start(counter()).unwrap();
    let reset = button(&app.view().unwrap(), "Reset");
    assert_eq!(
        app.update(&Event::Click { handler: reset }).unwrap(),
        [Command::SetInput {
            key: "name".into(),
            value: String::new()
        }]
    );
}

#[test]
fn a_list_draws_only_the_rows_asked_for() {
    let (mut app, _) = Runtime::start(counter()).unwrap();
    let view = app.view().unwrap();
    let list = find(&view, &|node| match node {
        Node::List(list) => Some(list.clone()),
        _ => None,
    })
    .unwrap();
    assert_eq!((list.key.as_str(), list.count), ("rows", 1000));

    let rows = app.rows("rows", 40..43).unwrap();
    let labels: Vec<_> = rows
        .iter()
        .map(|row| match row {
            Node::Button(button) => button.label.clone(),
            _ => panic!("a row is a button"),
        })
        .collect();
    assert_eq!(labels, ["Row 40", "Row 41", "Row 42"]);

    let Node::Button(row) = &rows[1] else {
        unreachable!()
    };
    app.update(&Event::Click {
        handler: row.on_click.unwrap(),
    })
    .unwrap();
    assert!(shows(&app.view().unwrap(), "Picked row 41"));
}

#[test]
fn a_handler_from_an_earlier_draw_is_dropped() {
    let (mut app, _) = Runtime::start(counter()).unwrap();
    let add = button(&app.view().unwrap(), "Add");
    app.view().unwrap();
    app.update(&Event::Click { handler: add }).unwrap();
    assert!(shows(&app.view().unwrap(), "0"));
}

#[test]
fn a_panic_is_the_apps_last_word() {
    let (mut app, _) = Runtime::start(counter()).unwrap();
    let panic = button(&app.view().unwrap(), "Panic");
    let error = app.update(&Event::Click { handler: panic }).unwrap_err();
    assert!(format!("{error:#}").contains("asked to panic"), "{error:#}");
}

#[test]
fn a_module_that_is_not_an_app_is_refused() {
    let error = Runtime::start(b"\0asm\x01\0\0\0").map(|_| ()).unwrap_err();
    assert!(
        format!("{error:#}").contains("exports no memory"),
        "{error:#}"
    );
}
