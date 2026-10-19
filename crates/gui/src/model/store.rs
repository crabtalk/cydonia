//! The backend a project's work is kept in.
//!
//! Every read and write of boards, sessions, articles, numbers and tables goes
//! through the [`Store`] that [`open`] answers, and `open` is the one place
//! that picks the backend: the memory project [`seed`] put at a location, else
//! the files at a location on this machine, else the project
//! [`crate::model::remote::Remotes`] has open at a location on another
//! machine. A memory project holds no tables.

pub use artifact::project::store::Store;
use artifact::{
    location::Location,
    project::{fs, memory},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

/// Projects held in memory, by the location they stand at, for the life of
/// the process.
fn seeded() -> &'static Mutex<HashMap<Location, Arc<memory::Project>>> {
    static SEEDED: OnceLock<Mutex<HashMap<Location, Arc<memory::Project>>>> = OnceLock::new();
    SEEDED.get_or_init(Default::default)
}

/// Stand `project` at `location`: every [`open`] of that location answers it
/// from here on, and nothing is read from or written to the disk there.
pub fn seed(location: impl Into<Location>, project: memory::Project) {
    seeded()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(location.into(), Arc::new(project));
}

/// `None` for a location on another machine that no project is open at.
pub fn open(location: &Location) -> Option<Store> {
    let held = seeded()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(location)
        .cloned();
    match (held, location) {
        (Some(project), _) => Some(Store::Memory(project)),
        (None, Location::Local(root)) => Some(Store::Fs(fs::Project::new(root))),
        #[cfg(feature = "desktop")]
        (None, Location::Remote { .. }) => crate::model::remote::Remotes::get()
            .ok()?
            .project(location)
            .map(Store::Remote),
    }
}
