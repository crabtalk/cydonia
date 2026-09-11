//! A scratch project, torn down when the test ends.

use cydonia_artifact::project;
use std::{fs, path::PathBuf};

pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cydonia-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// The store under it, which is what holds the entries.
    pub fn store(&self) -> project::fs::Project {
        project::fs::Project::new(&self.0)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
