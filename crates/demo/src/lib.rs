//! Cydonia in a browser tab: the same window the desktop app opens, on gpui's
//! web platform, over the welcome and tour projects held in memory.
//!
//! Single-threaded, like bezel's gallery. Sessions answer with a stand-in
//! rather than an agent; there is no terminal, settings window, updater or
//! tool server — see `cydonia-gui`'s `desktop` feature.

#![cfg(target_family = "wasm")]

use artifact::{
    project::{fs, memory},
    space::{Axis, Kind, Member, Node},
};
use bezel::gpui::{App, Application, ApplicationHandle};
use gui::{
    boot,
    model::{
        disk, language,
        settings::{Agent, Settings},
        spaces,
        state::State,
        store, welcome,
    },
    view::root,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};
use wasm_bindgen::prelude::wasm_bindgen;

include!(concat!(env!("OUT_DIR"), "/prepared.rs"));

/// The faces gpui-web resolves its defaults to: `.SystemUIFont` and `.ZedSans`
/// to IBM Plex Sans, `.ZedMono` to Lilex. The browser has no system fonts, and
/// text drawn outside the theme — a drag preview — asks for these by name.
const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-Italic.ttf"),
    include_bytes!("../assets/fonts/Lilex-Regular.ttf"),
    include_bytes!("../assets/fonts/Lilex-Bold.ttf"),
];

/// Where the welcome project stands. Nothing is on a disk there: the path is
/// the project's name in the sidebar and the key [`store::seed`] files it
/// under.
const PROJECT: &str = "/welcome";

/// The tour project, beside it. Its files are under `assets/showcase/`.
const TOUR: &str = "/tour";

/// A file of the tour project, named under `.cydonia/`, and its bytes.
macro_rules! tour {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!("../assets/showcase/.cydonia/", $path)) as &[u8],
        )
    };
}

const TOUR_FILES: [(&str, &[u8]); 4] = [
    tour!("articles/1790000000100/content.md"),
    tour!("articles/1790000000100/properties.toml"),
    tour!("boards/1790000000200.toml"),
    tour!("sessions/1790000000300.json"),
];

/// The tour's working tree, which the right panel's Files lists: named from
/// the project's root, beside `.cydonia/`.
const TOUR_TREE: [(&str, &[u8]); 4] = [
    ("README.md", include_bytes!("../assets/showcase/README.md")),
    (
        "Cargo.toml",
        include_bytes!("../assets/showcase/Cargo.toml"),
    ),
    (
        "src/main.rs",
        include_bytes!("../assets/showcase/src/main.rs"),
    ),
    (
        "src/notes.rs",
        include_bytes!("../assets/showcase/src/notes.rs"),
    ),
];

/// And the welcome project's.
const WELCOME_TREE: [(&str, &[u8]); 1] =
    [("README.md", include_bytes!("../assets/welcome/README.md"))];

thread_local! {
    /// The whole app, and the reason it stays alive: on wasm the run loop is
    /// the browser's, so `run_embedded` returns at once and hands back the one
    /// owner of everything it built.
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
}

/// Put `text` where the host page says it is starting, which is the only
/// place a reader looks.
fn show(text: &str) {
    if let Some(boot) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("boot"))
    {
        boot.set_text_content(Some(&format!("Cydonia did not start: {text}")));
    }
}

/// The tour's article beside its board, with the session behind the board's
/// cards under it.
fn tour_space() {
    let article = fs::Project::new(TOUR)
        .cydonia()
        .join("articles/1790000000100/content.md");
    let article = Member::new(TOUR, Kind::Article, article.to_string_lossy());
    let Some(mut space) = spaces::create("Tour", article.clone()) else {
        return;
    };
    space.tree = Node::split(
        Axis::Horizontal,
        vec![
            Node::leaf(article),
            Node::split(
                Axis::Vertical,
                vec![
                    Node::leaf(Member::new(TOUR, Kind::Board, "1790000000200")),
                    Node::leaf(Member::new(TOUR, Kind::Session, "1790000000300")),
                ],
            ),
        ],
    );
    spaces::save(&mut space);
}

#[wasm_bindgen(start)]
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        show(&info.to_string());
    }));
    gpui_web::init_logging();
    language::prepare(PREPARED);
    store::seed(PROJECT, memory::Project::seed(welcome::files()));
    store::seed(TOUR, memory::Project::seed(TOUR_FILES));
    tour_space();
    disk::seed(PROJECT.as_ref(), WELCOME_TREE);
    disk::seed(TOUR.as_ref(), TOUR_TREE);

    // The name the tour's session was filed under, so it reads back as one
    // this agent can answer.
    let settings = Settings {
        agents: vec![Agent {
            name: "Demo agent".into(),
            id: None,
            command: "demo".into(),
            args: Vec::new(),
            env: Default::default(),
        }],
        ..Settings::default()
    };
    let state = State {
        projects: vec![PROJECT.into(), TOUR.into()],
        last: [(PROJECT.into(), welcome::landing(PROJECT.as_ref()))].into(),
        ..State::default()
    };
    let platform = Rc::new(gpui_web::WebPlatform::new(false));
    let http_client = Arc::new(platform.fetch_http_client());
    let handle = Application::with_platform(platform)
        .with_http_client(http_client)
        .run_embedded(move |cx: &mut App| {
            if let Err(error) = cx
                .text_system()
                .add_fonts(FONTS.map(Cow::Borrowed).to_vec())
            {
                show(&format!("font registration failed: {error:?}"));
            }
            boot::init(&settings, cx);
            root::open(settings, state, cx).expect("failed to open the window");
        });
    APPLICATION.with(|application| *application.borrow_mut() = Some(handle));
}
