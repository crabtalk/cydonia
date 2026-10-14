//! Apps on the runner: `cydonia-guest`'s examples, built for
//! `wasm32-unknown-unknown` and driven through `Runner`.

use cydonia_gui::{
    data::Data,
    model::apps::{Answer, Call, Command, Event, Node, Runner as _, Runtime},
};
use guest::wire;
use std::{path::PathBuf, process::Command as Process, sync::OnceLock};

/// The `.wasm` of the example `name`. Every example is built once per test
/// run.
fn example(name: &str) -> Vec<u8> {
    static TARGET: OnceLock<PathBuf> = OnceLock::new();
    let target = TARGET.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("guest");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Process::new(cargo)
            .current_dir(&root)
            .args([
                "build",
                "-p",
                "cydonia-guest",
                "--examples",
                "--target",
                "wasm32-unknown-unknown",
                "--target-dir",
            ])
            .arg(&target)
            .status()
            .expect("cargo runs");
        assert!(status.success(), "the examples build for wasm32");
        target.join("wasm32-unknown-unknown/debug/examples")
    });
    std::fs::read(target.join(format!("{name}.wasm"))).expect("the example's wasm")
}

fn counter() -> Vec<u8> {
    example("counter")
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
    let (mut app, commands) = Runtime::start(&counter()).unwrap();
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
    let (mut app, _) = Runtime::start(&counter()).unwrap();
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
    let (mut app, _) = Runtime::start(&counter()).unwrap();
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
    let (mut app, _) = Runtime::start(&counter()).unwrap();
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
    let (mut app, _) = Runtime::start(&counter()).unwrap();
    let add = button(&app.view().unwrap(), "Add");
    app.view().unwrap();
    app.update(&Event::Click { handler: add }).unwrap();
    assert!(shows(&app.view().unwrap(), "0"));
}

#[test]
fn a_panic_is_the_apps_last_word() {
    let (mut app, _) = Runtime::start(&counter()).unwrap();
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

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("cydonia-apps-{name}-{}", std::process::id()));
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

/// Answer every call the app makes on `data`, and the calls those answers
/// lead to, until it makes none.
fn settle(app: &mut Runtime, data: &mut Data, mut commands: Vec<Command>) {
    while !commands.is_empty() {
        for command in std::mem::take(&mut commands) {
            let Command::Call { id, call } = command else {
                continue;
            };
            let answer = data
                .call(call)
                .unwrap_or_else(|error| Answer::Error(format!("{error:#}")));
            commands.extend(app.update(&Event::Answer { call: id, answer }).unwrap());
        }
    }
}

#[test]
fn an_app_makes_writes_and_reads_a_project_table() {
    let scratch = Scratch::new("notes");
    let mut data = Data::open(&scratch.0).unwrap();
    let (mut app, commands) = Runtime::start(&example("notes")).unwrap();
    assert!(matches!(
        commands.as_slice(),
        [Command::Call {
            call: Call::Tables,
            ..
        }]
    ));
    settle(&mut app, &mut data, commands);
    let tables = data.list().unwrap();
    assert_eq!(tables.len(), 1);
    assert_eq!(
        (tables[0].key.as_str(), tables[0].name.as_str()),
        ("notes", "Notes")
    );
    assert!(tables[0].number.is_some());

    for _ in 0..2 {
        let add = button(&app.view().unwrap(), "Add");
        let commands = app.update(&Event::Click { handler: add }).unwrap();
        settle(&mut app, &mut data, commands);
    }
    let view = app.view().unwrap();
    assert!(shows(&view, "Note 1") && shows(&view, "Note 2"));
    assert_eq!(data.list().unwrap()[0].rows, 2);

    let clear = button(&view, "Clear");
    let commands = app.update(&Event::Click { handler: clear }).unwrap();
    settle(&mut app, &mut data, commands);
    let view = app.view().unwrap();
    assert!(!shows(&view, "Note 1") && shows(&view, "Note 2"));
}

#[test]
fn an_app_finds_the_table_it_made_on_its_next_run() {
    let scratch = Scratch::new("rerun");
    let mut data = Data::open(&scratch.0).unwrap();
    for _ in 0..2 {
        let (mut app, commands) = Runtime::start(&example("notes")).unwrap();
        settle(&mut app, &mut data, commands);
    }
    assert_eq!(data.list().unwrap().len(), 1);
}

#[test]
fn an_insert_naming_a_missing_column_writes_nothing() {
    let scratch = Scratch::new("insert");
    let mut data = Data::open(&scratch.0).unwrap();
    data.call(Call::CreateTable {
        name: "Notes".into(),
        columns: vec![wire::Column {
            name: "text".into(),
            kind: wire::ColType::Text,
        }],
    })
    .unwrap();
    let good = serde_json::json!({ "text": "kept" });
    let bad = serde_json::json!({ "missing": 1 });
    let answer = data.call(Call::Insert {
        table: "notes".into(),
        rows: vec![
            good.as_object().unwrap().clone(),
            bad.as_object().unwrap().clone(),
        ],
    });
    assert!(answer.is_err());
    assert_eq!(data.list().unwrap()[0].rows, 0);
}

/// Put an app named `hello` in `project`'s `apps/`, its crate root `lib`.
fn write_app(project: &std::path::Path, lib: &str) {
    let dir = project.join("apps/hello");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let guest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../guest");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n\
             [lib]\ncrate-type = [\"cdylib\"]\n\n\
             [dependencies]\ncydonia-guest = {{ path = {:?} }}\n\n[workspace]\n",
            guest.canonicalize().unwrap()
        ),
    )
    .unwrap();
    std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
}

const HELLO: &str = r#"
use cydonia_guest::{App, node::*};

struct Hello;

impl App for Hello {
    type Msg = ();
    fn init() -> Self {
        Hello
    }
    fn update(&mut self, _: ()) {}
    fn view(&self) -> Node<()> {
        text("Hello").into()
    }
}

cydonia_guest::app!(Hello, name = "Hello there", calls = [tables]);
"#;

#[test]
fn an_app_in_apps_is_built_and_listed_by_its_manifest() {
    use artifact::{app::Source, entry, kind::Kind};
    use cydonia_gui::model::apps::Program;

    let scratch = Scratch::new("build");
    std::fs::create_dir_all(scratch.0.join("apps/not-a-crate")).unwrap();
    write_app(&scratch.0, HELLO);
    let apps = Source::list(&scratch.0);
    assert_eq!(apps, [Source::new(&scratch.0, "hello")]);
    assert_eq!(apps[0].title(), "hello");

    let (wasm, manifest) = Program::compile(&apps[0]).unwrap();
    assert_eq!(
        (manifest.id.as_str(), manifest.name.as_str()),
        ("hello", "Hello there")
    );
    assert!(manifest.calls("tables"));
    assert_eq!(apps[0].title(), "Hello there");
    let (mut app, _) = Runtime::start(&wasm).unwrap();
    assert!(shows(&app.view().unwrap(), "Hello"));

    let listed = entry::list(&scratch.0).unwrap();
    let found = listed.iter().find(|entry| entry.kind == Kind::App).unwrap();
    assert_eq!(
        (found.id.as_str(), found.title.as_str()),
        ("hello", "Hello there")
    );

    write_app(
        &scratch.0,
        &HELLO.replace(
            "Hello;\n\nimpl",
            "Hello;\n\nfn broken() -> u32 { \"no\" }\n\nimpl",
        ),
    );
    let error = format!("{:#}", Program::compile(&apps[0]).unwrap_err());
    assert!(error.contains("mismatched types"), "{error}");
    assert_eq!(apps[0].title(), "Hello there");
}

/// Answer one request on a local port with `response`, and hand back the URL
/// and what the request said.
fn serve_once(response: &'static str) -> (String, std::thread::JoinHandle<String>) {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/path",
        listener.local_addr().unwrap().port()
    );
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = vec![0; 4096];
        let read = stream.read(&mut request).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    (url, served)
}

#[test]
fn a_fetch_answers_with_status_headers_and_body() {
    use cydonia_gui::model::apps::{Fetcher as _, Http};
    let (url, served) =
        serve_once("HTTP/1.1 201 Created\r\nX-Kind: test\r\nContent-Length: 5\r\n\r\nhello");
    let response = Http::fetch(&wire::Request {
        method: "POST".into(),
        url,
        headers: vec![("x-from".into(), "app".into())],
        body: Some(b"ping".to_vec()),
    })
    .unwrap();
    assert_eq!(response.status, 201);
    assert_eq!(response.header("X-KIND"), Some("test"));
    assert_eq!(response.text(), "hello");
    let request = served.join().unwrap();
    assert!(request.starts_with("POST /path"), "{request}");
    assert!(request.contains("x-from: app"), "{request}");
}

#[test]
fn a_redirect_comes_back_unfollowed() {
    use cydonia_gui::model::apps::{Fetcher as _, Http};
    let (url, served) = serve_once(
        "HTTP/1.1 302 Found\r\nLocation: https://elsewhere.example/\r\nContent-Length: 0\r\n\r\n",
    );
    let response = Http::fetch(&wire::Request {
        method: "GET".into(),
        url,
        headers: Vec::new(),
        body: None,
    })
    .unwrap();
    assert_eq!(response.status, 302);
    assert_eq!(
        response.header("location"),
        Some("https://elsewhere.example/")
    );
    served.join().unwrap();
}
