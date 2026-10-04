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
        settings::{Agent, Appearance, Settings},
        spaces,
        state::{self, State},
        store, welcome,
    },
    view::root,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};
use wasm_bindgen::prelude::wasm_bindgen;

include!(concat!(env!("OUT_DIR"), "/prepared.rs"));

/// The faces the demo draws with: Inter, set as the interface family in
/// [`start`], and Lilex, which gpui-web resolves `.ZedMono` to. The browser
/// has no system fonts. gpui-web resolves `.SystemUIFont` and `.ZedSans` to
/// IBM Plex Sans, which is not bundled, so text drawn outside the theme — a
/// drag preview — falls back.
const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Italic.ttf"),
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

/// The tour's article, by the id the app names it with: its path.
fn tour_article() -> String {
    fs::Project::new(TOUR)
        .cydonia()
        .join("articles/1790000000100/content.md")
        .to_string_lossy()
        .into_owned()
}

/// The tour's article beside its board, with the session behind the board's
/// cards under it. Returns the space's id.
fn tour_space() -> Option<String> {
    let article = Member::new(TOUR, Kind::Article, tour_article());
    let mut space = spaces::create("Tour", article.clone())?;
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
    Some(space.id)
}

/// The `show` parameter of the page's query: what the window opens on, for a
/// host page that frames one entry at a time. `article`, `board`, `session`
/// and `space` open the tour's; anything else, or none, the welcome project.
fn shown() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    search
        .trim_start_matches('?')
        .split('&')
        .find_map(|pair| pair.strip_prefix("show="))
        .map(str::to_owned)
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
    let space = tour_space();
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
        appearance: Appearance {
            ui_font: Some("Inter".into()),
            ..Default::default()
        },
        ..Settings::default()
    };
    let mut state = State {
        projects: vec![PROJECT.into(), TOUR.into()],
        last: [(PROJECT.into(), welcome::landing(PROJECT.as_ref()))].into(),
        ..State::default()
    };
    let tour = |kind, id: String| state::Entry { kind, id };
    let entry = match shown().as_deref() {
        Some("article") => Some(tour(state::Kind::Article, tour_article())),
        Some("board") => Some(tour(state::Kind::Board, "1790000000200".into())),
        Some("session") => Some(tour(state::Kind::Session, "1790000000300".into())),
        Some("space") => {
            state.space = space;
            None
        }
        _ => None,
    };
    if let Some(entry) = entry {
        state.active = 1;
        state.last.insert(TOUR.into(), entry);
    }
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
