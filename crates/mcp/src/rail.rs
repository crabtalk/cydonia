//! The projects the app has open, from a tool's side of the glass.
//!
//! Everything else here is a directory and a file layout, which is why the rest
//! of this crate holds nothing at all: a project is wherever the call says it
//! is, opened or not. Which projects a *window* is showing is not like that —
//! it is the app's own list, in memory, rewritten whole every time it moves —
//! so this is the one place the two have to meet.
//!
//! The app installs what to do about a change and pushes what it is holding; a
//! tool asks, and reads. **Asked rather than done**: a call arrives on whichever
//! thread the door is serving from, and the rail can only be moved on the one
//! the window is drawn on. So a tool hands its change over and answers for the
//! part it did itself — the directory — which is the part that can fail.
//!
//! Installed once at launch, like a highlighter or a link preview. A build with
//! nothing installed — the tests, a caller that mounted the tool sets on their
//! own — refuses rather than pretends.

#[cfg(feature = "http")]
use crate::tool::Trouble;
use std::{
    path::{Path, PathBuf},
    sync::RwLock,
};

/// What a tool asks of the rail. Closing takes a project off it and stops the
/// agents running in it; nothing on disk is touched either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Open(PathBuf),
    Close(PathBuf),
    /// A prompt for the session filed under `session` — its record id, which
    /// is unique across projects.
    Send {
        session: String,
        message: String,
    },
    /// A new session in `project` on the agent `settings.toml` names `agent`,
    /// with `message` as its first prompt.
    Start {
        project: PathBuf,
        agent: String,
        message: String,
    },
}

/// An agent a session can be started on, as `settings.toml` files it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub name: String,
    /// The registry id, absent for an agent added by hand.
    pub id: Option<String>,
}

impl Agent {
    /// What a tool names it by: the id where there is one, else the name.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.name)
    }
}

/// An entry the window is showing: an article, a board or a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub project: PathBuf,
    /// `article`, `board` or `table`, as [`artifact::entry::Entry::kind`]
    /// spells it.
    pub kind: &'static str,
    pub id: String,
    /// Whether it is the pane with the focus. Always set on a lone entry.
    pub focused: bool,
}

type Hand = Box<dyn Fn(Change) + Send + Sync>;

/// What the app does about a change.
static HAND: RwLock<Option<Hand>> = RwLock::new(None);

/// What the app is holding, as it last left it.
static OPEN: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// The agents configured, as the app last pushed them.
static AGENTS: RwLock<Vec<Agent>> = RwLock::new(Vec::new());

/// The entries on screen, as the window last pushed them.
static SHOWN: RwLock<Vec<Shown>> = RwLock::new(Vec::new());

/// Hand the rail over. The app calls this once, at launch.
pub fn install(hand: impl Fn(Change) + Send + Sync + 'static) {
    if let Ok(mut held) = HAND.write() {
        *held = Some(Box::new(hand));
    }
}

/// Say what is on the rail. Pushed by the app wherever the list is written
/// down, rather than read back off a file: `state.toml` is the app's to rewrite
/// whole, and a tool reading it would be racing the next save.
pub fn set_open(projects: Vec<PathBuf>) {
    if let Ok(mut held) = OPEN.write() {
        *held = projects;
    }
}

/// The projects on the rail, in the order the app lists them.
pub fn open() -> Vec<PathBuf> {
    OPEN.read().map(|held| held.clone()).unwrap_or_default()
}

/// Say which agents are configured. Pushed by the app whenever it reads
/// `settings.toml`.
pub fn set_agents(agents: Vec<Agent>) {
    if let Ok(mut held) = AGENTS.write() {
        *held = agents;
    }
}

/// The agents configured, in the order `settings.toml` lists them.
pub fn agents() -> Vec<Agent> {
    AGENTS.read().map(|held| held.clone()).unwrap_or_default()
}

/// Say what the window is showing. Pushed on every render of the window, so
/// a write happens only when the list changed.
pub fn set_shown(shown: Vec<Shown>) {
    let same = SHOWN.read().is_ok_and(|held| *held == shown);
    if !same && let Ok(mut held) = SHOWN.write() {
        *held = shown;
    }
}

/// The entries on screen, in the order the window lays them out. Empty when
/// the window is on a chat, or has nothing open.
pub fn shown() -> Vec<Shown> {
    SHOWN.read().map(|held| held.clone()).unwrap_or_default()
}

/// Whether the rail is holding a project at `path`.
///
/// Either spelling of it counts. The sidebar holds whatever the directory
/// picker was given and the tools settle a path before asking about one, so
/// `/tmp/x` and `/private/tmp/x` reach here as one project — the app resolves
/// the same pair on its side, in `Workspace::project_at`.
pub fn is_open(path: &Path) -> bool {
    let settled = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let want = settled(path);
    open()
        .iter()
        .any(|held| held == path || settled(held) == want)
}

/// Ask for the change, which is as far as a tool can take it.
#[cfg(feature = "http")]
pub(crate) fn ask(change: Change) -> Result<(), Trouble> {
    let held = HAND.read().map_err(|_| nobody())?;
    let Some(hand) = held.as_ref() else {
        return Err(nobody());
    };
    hand(change);
    Ok(())
}

#[cfg(feature = "http")]
fn nobody() -> Trouble {
    Trouble::Refused("no cydonia window to open a project in".to_owned())
}

/// What a browser tool asks of a project's right-panel browser.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// The project's browser tabs.
    Tabs,
    /// Load `url` in the tab named, or in a new one, and read it.
    Open(String),
    Read,
    Click(usize),
    Type {
        element: usize,
        text: String,
        enter: bool,
    },
    /// Screenfuls down; negative goes up.
    Scroll(f64),
}

/// One browser call, with where its answer goes.
pub struct Browse {
    pub project: PathBuf,
    /// A tab id from [`Act::Tabs`]. `None` is the panel's front browser tab,
    /// except for [`Act::Open`], where it is a new tab.
    pub tab: Option<u64>,
    pub act: Act,
    /// The text the model reads, or why there is none.
    pub reply: std::sync::mpsc::Sender<Result<String, String>>,
}

type Browser = Box<dyn Fn(Browse) + Send + Sync>;

static BROWSER: RwLock<Option<Browser>> = RwLock::new(None);

/// Hand the browser over. The app calls this once, at launch.
pub fn install_browser(hand: impl Fn(Browse) + Send + Sync + 'static) {
    if let Ok(mut held) = BROWSER.write() {
        *held = Some(Box::new(hand));
    }
}

/// How long a browser call waits for the window's answer.
#[cfg(feature = "http")]
const BROWSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Ask the window's browser, and wait for its answer.
///
/// Blocks the calling thread. Never call it from the thread the window is
/// drawn on: the answer is made there.
#[cfg(feature = "http")]
pub(crate) fn browse(project: PathBuf, tab: Option<u64>, act: Act) -> Result<String, Trouble> {
    let (reply, answer) = std::sync::mpsc::channel();
    {
        let held = BROWSER.read().map_err(|_| no_browser())?;
        let Some(hand) = held.as_ref() else {
            return Err(no_browser());
        };
        hand(Browse {
            project,
            tab,
            act,
            reply,
        });
    }
    match answer.recv_timeout(BROWSE_TIMEOUT) {
        Ok(answer) => answer.map_err(Trouble::Refused),
        Err(_) => Err(Trouble::Refused(
            "the browser did not answer in time".to_owned(),
        )),
    }
}

#[cfg(feature = "http")]
fn no_browser() -> Trouble {
    Trouble::Refused("no cydonia window with a browser to work in".to_owned())
}
