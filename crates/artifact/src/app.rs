//! A project's apps. Each directory under `apps/` holding a `Cargo.toml` is a
//! crate built against `cydonia-guest`, named by its directory; what building
//! one gives is kept under `.cydonia/build/`. An app installed from a registry
//! is applied to a project by its record under `.cydonia/apps/<id>/`, and its
//! `.wasm` is kept once for every project, under the data directory's `app/`.

use crate::project::{
    Watching,
    fs::{self, component},
};
use anyhow::{Context as _, Result, bail};
use guest::manifest::Manifest;
use notify::{RecursiveMode, Watcher as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The directory a project's apps are in, and under `.cydonia/` the one its
/// installed apps are applied in.
pub const DIR: &str = "apps";
/// Under `.cydonia/`: the built apps, and the target directory they are
/// built in.
const BUILD: &str = "build";
/// An installed app's record, in its directory under `.cydonia/apps/`.
const RECORD: &str = "app.toml";

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

/// An app installed from a registry and applied to a project: where it was
/// pulled from, and the digest its `.wasm` is pinned to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    /// Its manifest's id, which names its directories.
    pub id: String,
    /// The repository it was pulled from, without a tag:
    /// `ghcr.io/crabtalk/cydonia/yc`. What the project lists it by.
    pub reference: String,
    /// The tag it was pulled by.
    pub tag: String,
    /// The `.wasm`'s digest, `sha256:<hex>`.
    pub digest: String,
    /// The digest the person has let run in this project, after its
    /// permission prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<String>,
}

impl Installed {
    /// The installed apps applied to `project`, by reference.
    pub fn list(project: &Path) -> Vec<Self> {
        let Ok(dirs) = std::fs::read_dir(applied_dir(project)) else {
            return Vec::new();
        };
        let mut apps: Vec<Self> = dirs
            .flatten()
            .filter_map(|dir| std::fs::read_to_string(dir.path().join(RECORD)).ok())
            .filter_map(|text| toml::from_str(&text).ok())
            .collect();
        apps.sort_by(|a: &Self, b| a.reference.cmp(&b.reference));
        apps
    }

    /// Apply it to `project`, or write what changed about it there. An app
    /// with another reference already applied under the same id is refused.
    pub fn save(&self, project: &Path) -> Result<()> {
        let path = applied_dir(project).join(component(&self.id)?).join(RECORD);
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(held) = toml::from_str::<Self>(&text)
            && held.reference != self.reference
        {
            bail!(
                "{} is already applied here from {}",
                self.id,
                held.reference
            );
        }
        crate::fs::save(&path, toml::to_string(self)?)?;
        Ok(())
    }

    /// Whether the person has let this digest run.
    pub fn allowed(&self) -> bool {
        self.allowed.as_deref() == Some(self.digest.as_str())
    }

    /// Where its pinned `.wasm` is kept.
    pub fn wasm(&self) -> Result<PathBuf> {
        Self::wasm_at(&self.id, &self.digest)
    }

    /// Where the `.wasm` of app `id` with `digest` is kept.
    pub fn wasm_at(id: &str, digest: &str) -> Result<PathBuf> {
        let hex = digest
            .strip_prefix("sha256:")
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .with_context(|| format!("{digest} is not a sha256 digest"))?;
        Ok(crate::backup::data_dir()?
            .join("app")
            .join(component(id)?)
            .join(format!("{hex}.wasm")))
    }

    /// Its manifest, read from its kept `.wasm`.
    pub fn manifest(&self) -> Option<Manifest> {
        Manifest::read(&std::fs::read(self.wasm().ok()?).ok()?).ok()?
    }
}

fn applied_dir(project: &Path) -> PathBuf {
    fs::Project::new(project).cydonia().join(DIR)
}
