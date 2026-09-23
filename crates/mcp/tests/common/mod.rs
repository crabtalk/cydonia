//! A scratch project, and a server over it, torn down when the test ends.

// Every test binary compiles this module whole and reaches for a part of it, so
// what one of them leaves alone is not dead.
#![allow(dead_code)]

use artifact::project;
use cydonia_mcp::{
    Server,
    rail::{self, Change},
    tool::{Outcome, Trouble},
    tools,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cydonia-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The store under it, which is what the app writes through too.
    pub fn store(&self) -> project::fs::Project {
        project::fs::Project::new(&self.0)
    }

    /// Put a board in it, the way the app's own store does.
    pub fn store_create(&self, name: &str, key: &str) -> Option<artifact::board::Board> {
        use artifact::project::Project as _;
        self.store().create_board(name, key)
    }

    /// A server with every tool set on it — the same call the app makes, and
    /// holding no project, because a call says which one it is about.
    ///
    /// Puts the scratch directory on the rail, which is what a call naming a
    /// project needs: the tools reach the projects cydonia has open, and a
    /// test that skipped this would be testing a directory the app never
    /// opened. Left on beside whatever else is held, so a test with two
    /// scratches can have a call name the one it is not bound to.
    /// Every door open. What each switch withholds is its own test — see
    /// `a_read_only_server_offers_no_way_to_write` and
    /// `deleting_is_withheld_until_its_own_switch_is_on` — and a helper that
    /// held one shut would make every other test about the switch.
    pub fn server(&self) -> Server {
        Rail::also(self.path());
        Server::new()
            .mount(&tools::article::TOOLS)
            .mount(&tools::board::TOOLS)
            .mount(&tools::project::TOOLS)
            .mount(&tools::session::TOOLS)
            .deletes(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                true,
            )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A rail with nobody's window behind it: what the tools asked for, in the
/// order they asked, and whatever the app is meant to be holding.
///
/// A process-wide pair, which is what the rail is — so a test asserts about the
/// paths it made itself rather than about the length of the list. Under
/// `cargo nextest` each test has the statics to itself anyway.
pub struct Rail;

static ASKED: Mutex<Vec<Change>> = Mutex::new(Vec::new());

impl Rail {
    /// Install the recorder, and say what the app is holding.
    pub fn holding(open: &[&Path]) -> Self {
        rail::install(|change| ASKED.lock().unwrap().push(change));
        rail::set_open(open.iter().map(PathBuf::from).collect());
        Self
    }

    /// Put one more project on the rail, leaving what is already there.
    pub fn also(path: &Path) {
        rail::install(|change| ASKED.lock().unwrap().push(change));
        let mut open = rail::open();
        if !open.iter().any(|held| held == path) {
            open.push(path.to_path_buf());
        }
        rail::set_open(open);
    }

    pub fn asked(&self) -> Vec<Change> {
        ASKED.lock().unwrap().clone()
    }

    pub fn was_asked(&self, change: Change) -> bool {
        self.asked().contains(&change)
    }
}

/// What a call that worked said.
pub fn said(outcome: Outcome) -> String {
    match outcome {
        Ok(answer) => answer.text,
        Err(Trouble::Refused(why)) => panic!("refused: {why}"),
        Err(Trouble::Invalid(why)) => panic!("invalid: {why}"),
    }
}

/// What a call that was told no said. The distinction from [`invalid`] is the
/// point: one reaches the model as a result it can act on, the other as a
/// protocol error it cannot.
pub fn refused(outcome: Outcome) -> String {
    match outcome {
        Err(Trouble::Refused(why)) => why,
        Err(Trouble::Invalid(why)) => panic!("invalid, not refused: {why}"),
        Ok(answer) => panic!("it worked: {}", answer.text),
    }
}

/// What a call answered beside its words — the `data` a tool puts structured
/// results in.
pub fn structured(outcome: Outcome) -> serde_json::Value {
    match outcome {
        Ok(answer) => answer.data.expect("the call answered with data"),
        Err(Trouble::Refused(why)) => panic!("refused: {why}"),
        Err(Trouble::Invalid(why)) => panic!("invalid: {why}"),
    }
}

pub fn invalid(outcome: Outcome) -> String {
    match outcome {
        Err(Trouble::Invalid(why)) => why,
        Err(Trouble::Refused(why)) => panic!("refused, not invalid: {why}"),
        Ok(answer) => panic!("it worked: {}", answer.text),
    }
}
