//! A project's apps: each directory under `apps/` holding a `Cargo.toml` is a
//! crate built against `cydonia-guest`, named by its directory. What building
//! one gives is kept under `.cydonia/build/`.

use crate::project::{Watching, fs};
use guest::manifest::Manifest;
use notify::{RecursiveMode, Watcher as _};
use std::path::{Path, PathBuf};

/// The directory a project's apps are in.
pub const DIR: &str = "apps";
/// Under `.cydonia/`: the built apps, and the target directory they are
/// built in.
const BUILD: &str = "build";

/// One app's source, by the project it is in and its directory's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub project: PathBuf,
    pub name: String,
}

impl Source {
    pub fn new(project: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            name: name.into(),
        }
    }

    /// The project's apps, by name.
    pub fn list(project: &Path) -> Vec<Self> {
        let Ok(dirs) = std::fs::read_dir(project.join(DIR)) else {
            return Vec::new();
        };
        let mut apps: Vec<Self> = dirs
            .flatten()
            .filter(|dir| dir.path().join("Cargo.toml").is_file())
            .filter_map(|dir| Some(Self::new(project, dir.file_name().to_str()?)))
            .collect();
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        apps
    }

    /// The crate's directory.
    pub fn dir(&self) -> PathBuf {
        self.project.join(DIR).join(&self.name)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.dir().join("Cargo.toml")
    }

    /// The `.wasm` the last good build left.
    pub fn built(&self) -> PathBuf {
        build_dir(&self.project).join(format!("{}.wasm", self.name))
    }

    /// The target directory every app of the project is built in.
    pub fn target(&self) -> PathBuf {
        build_dir(&self.project).join("target")
    }

    /// The manifest of the last good build. `None` before the first one.
    pub fn manifest(&self) -> Option<Manifest> {
        Manifest::read(&std::fs::read(self.built()).ok()?).ok()?
    }

    /// What the app is listed as: its manifest's name, or its directory's
    /// before it is built.
    pub fn title(&self) -> String {
        self.manifest()
            .map(|manifest| manifest.name)
            .unwrap_or_else(|| self.name.clone())
    }
}

fn build_dir(project: &Path) -> PathBuf {
    fs::Project::new(project).cydonia().join(BUILD)
}

/// Watch a project's `apps/`, calling `changed` with the name of each app a
/// change lands in, its `target/` left out. A project with no `apps/` is
/// watched shallowly at its root, unsettled, and `changed` hears an empty name
/// when `apps/` appears.
pub fn watch(project: &Path, changed: impl Fn(&str) + Send + Sync + 'static) -> Option<Watching> {
    let root = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let apps = root.join(DIR);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        for path in &event.paths {
            if path == &apps {
                changed("");
                continue;
            }
            let Ok(rest) = path.strip_prefix(&apps) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(name) = parts.next().and_then(|name| name.as_os_str().to_str()) else {
                continue;
            };
            let in_target = parts
                .next()
                .is_some_and(|part| part.as_os_str() == "target");
            if !in_target && !crate::fs::is_temporary(path) {
                changed(name);
            }
        }
    })
    .ok()?;
    match watcher.watch(&project.join(DIR), RecursiveMode::Recursive) {
        Ok(()) => Some(Watching::new(true, watcher)),
        Err(_) => watcher
            .watch(project, RecursiveMode::NonRecursive)
            .ok()
            .map(|()| Watching::new(false, watcher)),
    }
}
