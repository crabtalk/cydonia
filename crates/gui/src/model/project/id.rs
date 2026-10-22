//! What names a project inside the app: an id per directory, handed out the
//! first time a path to it is seen and kept for the life of the process.
//!
//! A path becomes an id where it comes in — a file read, a tool, the picker —
//! and an id becomes a path where one goes out: a file written, a process
//! spawned.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

/// A project, whether it is open or not. Two paths that resolve to one
/// directory are one id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(u32);

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Every project path seen, and the id each was given.
#[derive(Default)]
pub struct ProjectIds {
    /// By id: the path it was first seen under.
    paths: Vec<PathBuf>,
    /// Every path seen, as given and as the filesystem resolves it.
    ids: HashMap<PathBuf, ProjectId>,
}

impl ProjectIds {
    /// The id of the project at `path`, given one where it has none.
    ///
    /// Resolves `path` on the filesystem the first time it is seen, and never
    /// again.
    pub fn intern(&mut self, path: &Path) -> ProjectId {
        if let Some(id) = self.ids.get(path) {
            return *id;
        }
        let settled = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let id = match self.ids.get(&settled) {
            Some(id) => *id,
            None => {
                let id = ProjectId(self.paths.len() as u32);
                self.paths.push(path.to_path_buf());
                self.ids.insert(settled, id);
                id
            }
        };
        self.ids.insert(path.to_path_buf(), id);
        id
    }

    /// The id of `path`, where it has been seen as given or as it resolves.
    /// Makes no filesystem call.
    pub fn find(&self, path: &Path) -> Option<ProjectId> {
        self.ids.get(path).copied()
    }

    /// The path `id` was first seen under.
    pub fn path(&self, id: ProjectId) -> &Path {
        &self.paths[id.0 as usize]
    }

    /// `held`, keyed by project id rather than by path. Two paths to one
    /// project are one key, the later's value kept.
    pub fn intern_keys<V>(&mut self, held: BTreeMap<PathBuf, V>) -> BTreeMap<ProjectId, V> {
        held.into_iter()
            .map(|(path, value)| (self.intern(&path), value))
            .collect()
    }

    /// `held`, keyed by path as a file writes it.
    pub fn path_keys<V: Clone>(&self, held: &BTreeMap<ProjectId, V>) -> BTreeMap<PathBuf, V> {
        held.iter()
            .map(|(id, value)| (self.path(*id).to_path_buf(), value.clone()))
            .collect()
    }
}
