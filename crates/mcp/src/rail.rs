//! The projects the app has open, from a tool's side of the glass.
//!
//! Everything else here is a directory and a file layout, which is why the rest
//! of this crate holds nothing at all: a project is wherever the call says it
//! is, opened or not. Which projects a *window* is showing is not like that —
//! it is the app's own list, in memory, rewritten whole every time it moves —
//! so this is the one place the two have to meet.
//!
//! The rail hands tool requests across to the window's thread. Changes are
//! queued. Session creation waits for the workspace to return the new
//! session's identity.
//!
//! Installed once at launch, like a highlighter or a link preview. A build with
//! nothing installed — the tests, a caller that mounted the tool sets on their
//! own — refuses rather than pretends.

use crate::tool::Answer;
#[cfg(feature = "http")]
use crate::tool::Trouble;
use artifact::{board::View, kind::Kind, session::mode::Mode};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{RwLock, mpsc},
};

/// What the app does about a change.
static HAND: RwLock<Option<Hand>> = RwLock::new(None);

/// What the app is holding, as it last left it.
static OPEN: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// The agents configured, as the app last pushed them.
static AGENTS: RwLock<Vec<Agent>> = RwLock::new(Vec::new());

/// The entries on screen, as the window last pushed them.
static SHOWN: RwLock<Vec<Shown>> = RwLock::new(Vec::new());

static BROWSER: RwLock<Option<Browser>> = RwLock::new(None);

/// How a new board is laid out, as the app last pushed it.
static BOARD_VIEW: RwLock<View> = RwLock::new(View::Lanes);

/// How long a browser call waits for the window's answer.
#[cfg(feature = "http")]
const BROWSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

#[cfg(feature = "http")]
const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
    /// A new title for the session filed under `session`: in the user's slot
    /// where the user named it, else in the agent's.
    Rename {
        session: String,
        title: String,
    },
    /// The labels the session filed under `session` carries, replacing what
    /// it had. Normalised.
    Label {
        session: String,
        labels: Vec<String>,
    },
}

pub enum Request {
    Change(Change),
    Start(Start),
}

pub struct Start {
    pub project: PathBuf,
    pub agent: String,
    pub message: String,
    pub reply: mpsc::Sender<Result<Started, String>>,
}

pub struct Started {
    pub id: String,
    /// Terminal runs have an in-process id and no project entry number.
    pub number: Option<u64>,
}

/// A named agent or installed ACP program offered for session creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    pub name: String,
    /// An identity UUID or registry id; absent for a program added by hand.
    pub id: Option<String>,
    pub workspace: Option<PathBuf>,
    pub mode: Mode,
}

impl Agent {
    pub fn in_project(&self, project: &Path) -> bool {
        self.workspace
            .as_deref()
            .is_none_or(|workspace| workspace == project)
    }

    /// The identity or registry id, falling back to the program name.
    pub fn key(&self) -> &str {
        self.id.as_deref().unwrap_or(&self.name)
    }
}

/// An entry the window is showing: an article, a board or a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub project: PathBuf,
    /// Never [`Kind::Session`].
    pub kind: Kind,
    pub id: String,
    /// Whether it is the pane with the focus. Always set on a lone entry.
    pub focused: bool,
}

type Hand = Box<dyn Fn(Request) + Send + Sync>;

/// Hand the rail over. The app calls this once, at launch.
pub fn install(hand: impl Fn(Request) + Send + Sync + 'static) {
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

/// Replace the offered agents and normalize their workspace paths.
pub fn set_agents(mut agents: Vec<Agent>) {
    for agent in &mut agents {
        if let Some(workspace) = &mut agent.workspace {
            *workspace = workspace
                .canonicalize()
                .unwrap_or_else(|_| workspace.clone());
        }
    }
    if let Ok(mut held) = AGENTS.write() {
        *held = agents;
    }
}

/// The offered agents, in snapshot order.
pub fn agents() -> Vec<Agent> {
    AGENTS.read().map(|held| held.clone()).unwrap_or_default()
}

/// Say how a new board is laid out. Pushed by the app at launch and whenever
/// the setting moves.
pub fn set_board_view(view: View) {
    if let Ok(mut held) = BOARD_VIEW.write() {
        *held = view;
    }
}

/// How a board a tool makes is laid out. [`View::Lanes`] until the app pushes
/// its setting.
pub fn board_view() -> View {
    BOARD_VIEW.read().map(|held| *held).unwrap_or_default()
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
/// the same pair on its side, in `ProjectIds::intern`.
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
    dispatch(Request::Change(change))
}

/// Blocks the calling thread until the workspace answers. Call off the UI thread.
#[cfg(feature = "http")]
pub(crate) fn start(project: PathBuf, agent: String, message: String) -> Result<Started, Trouble> {
    let (reply, answer) = mpsc::channel();
    dispatch(Request::Start(Start {
        project,
        agent,
        message,
        reply,
    }))?;
    match answer.recv_timeout(START_TIMEOUT) {
        Ok(answer) => answer.map_err(Trouble::Refused),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Trouble::Refused(
            "Cydonia did not confirm session creation in time".to_owned(),
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Trouble::Refused(
            "Cydonia could not start the session".to_owned(),
        )),
    }
}

#[cfg(feature = "http")]
fn dispatch(request: Request) -> Result<(), Trouble> {
    let held = HAND.read().map_err(|_| nobody())?;
    let Some(hand) = held.as_ref() else {
        return Err(nobody());
    };
    hand(request);
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
    Screenshot,
    Evaluate(String),
    Inspect(Target),
    Click(usize),
    Type {
        element: usize,
        text: String,
        enter: bool,
    },
    /// Screenfuls down; negative goes up.
    Scroll(f64),
    /// What the tab's page has logged.
    Console {
        levels: Vec<Level>,
        since: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Element(usize),
    Selector(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Log,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const ALL: [Self; 5] = [Self::Debug, Self::Log, Self::Info, Self::Warn, Self::Error];
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Debug => "debug",
            Self::Log => "log",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        })
    }
}

/// One browser call, with where its answer goes.
pub struct Browse {
    pub project: PathBuf,
    /// A tab id from [`Act::Tabs`]. `None` is the panel's front browser tab,
    /// except for [`Act::Open`], where it is a new tab.
    pub tab: Option<u64>,
    pub act: Act,
    /// The tool content, or why it is unavailable.
    pub reply: mpsc::Sender<Result<Answer, String>>,
}

type Browser = Box<dyn Fn(Browse) + Send + Sync>;

/// Hand the browser over. The app calls this once, at launch.
pub fn install_browser(hand: impl Fn(Browse) + Send + Sync + 'static) {
    if let Ok(mut held) = BROWSER.write() {
        *held = Some(Box::new(hand));
    }
}

/// Ask the window's browser, and wait for its answer.
///
/// Blocks the calling thread. Never call it from the thread the window is
/// drawn on: the answer is made there.
#[cfg(feature = "http")]
pub(crate) fn browse(project: PathBuf, tab: Option<u64>, act: Act) -> Result<Answer, Trouble> {
    let (reply, answer) = mpsc::channel();
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
