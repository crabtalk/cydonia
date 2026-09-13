//! A scratch project, and a server over it, torn down when the test ends.

// Every test binary compiles this module whole and reaches for a part of it, so
// what one of them leaves alone is not dead.
#![allow(dead_code)]

use artifact::project;
use cydonia_mcp::{
    Server,
    tool::{Outcome, Trouble},
    tools,
};
use std::{
    fs,
    path::{Path, PathBuf},
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

    /// A server with the board tools on it — the same call the app makes, and
    /// holding no project, because a call says which one it is about.
    pub fn server(&self) -> Server {
        Server::new().mount(&tools::board::TOOLS)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

pub fn invalid(outcome: Outcome) -> String {
    match outcome {
        Err(Trouble::Invalid(why)) => why,
        Err(Trouble::Refused(why)) => panic!("refused, not invalid: {why}"),
        Ok(answer) => panic!("it worked: {}", answer.text),
    }
}
