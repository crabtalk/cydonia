//! A project's apps, each an [`Origin`]: a crate in its `apps/`, named by its
//! directory; a crate anywhere on disk, loaded from its folder; or an app
//! installed from a registry. Cydonia builds the first two, each a [`Crate`],
//! and keeps what building one gives under `.cydonia/build/`. A loaded or an
//! installed app is applied to a project by its record under
//! `.cydonia/apps/<id>/`, and an installed app's `.wasm` is kept once for every
//! project, under the data directory's `app/`.

use crate::project::{
    Watching,
    fs::{self, component},
};
use anyhow::{Context as _, Result, bail};
use manifest::Manifest;
use notify::{RecursiveMode, Watcher as _};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

/// The directory a project's apps are in, and under `.cydonia/` the one its
/// loaded and installed apps are applied in.
pub const DIR: &str = "apps";
/// Under `.cydonia/`: the built apps, and the target directory they are
/// built in.
const BUILD: &str = "build";
/// Under `.cydonia/build/`: the built apps loaded from a folder.
const LOADED: &str = "loaded";
/// A loaded or installed app's record, in its directory under
/// `.cydonia/apps/`.
const RECORD: &str = "app.toml";
/// The directory cargo builds into, inside a crate.
const TARGET: &str = "target";

/// Where one of a project's apps comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A crate in the project's `apps/`.
    Source(Source),
    /// A crate loaded from a folder into the project at `project`.
    Loaded { project: PathBuf, app: Loaded },
    /// An app installed from a registry, applied to the project at `project`.
    Installed { project: PathBuf, app: Installed },
}

impl Origin {
    /// The project's apps: the crates in its `apps/` by name, then what is
    /// applied to it, by what it was applied from.
    pub fn list(project: &Path) -> Vec<Self> {
        let sources = Source::list(project).into_iter().map(Self::Source);
        let applied = Record::list(project)
            .into_iter()
            .map(|record| match record {
                Record::Loaded(app) => Self::Loaded {
                    project: project.to_path_buf(),
                    app,
                },
                Record::Installed(app) => Self::Installed {
                    project: project.to_path_buf(),
                    app,
                },
            });
        sources.chain(applied).collect()
    }

    /// What the project lists it by: a crate's directory name, a loaded
    /// app's folder, or an installed app's reference.
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Self::Source(source) => Cow::Borrowed(&source.name),
            Self::Loaded { app, .. } => app.name(),
            Self::Installed { app, .. } => Cow::Borrowed(&app.reference),
        }
    }

    /// The crate Cydonia builds it from. `None` for an installed app.
    pub fn local(&self) -> Option<Crate> {
        match self {
            Self::Source(source) => Some(source.krate()),
            Self::Loaded { project, app } => Some(app.krate(project)),
            Self::Installed { .. } => None,
        }
    }

    /// Whether the app may run without asking first. An installed app asks
    /// for each digest it runs.
    pub fn allowed(&self) -> bool {
        match self {
            Self::Source(_) | Self::Loaded { .. } => true,
            Self::Installed { app, .. } => app.allowed(),
        }
    }

    /// Its manifest: an installed app's, or a crate's from its last good
    /// build or, before the first one, its `Cargo.toml`.
    pub fn manifest(&self) -> Option<Manifest> {
        match self {
            Self::Installed { app, .. } => app.manifest(),
            local => local.local()?.manifest(),
        }
    }

    /// What the app is listed as: its manifest's name, or what the project
    /// lists it by.
    pub fn title(&self) -> String {
        self.manifest()
            .map(|manifest| manifest.name)
            .unwrap_or_else(|| self.name().into_owned())
    }
}

/// A crate Cydonia builds into an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crate {
    /// Its directory, holding its `Cargo.toml`.
    pub dir: PathBuf,
    /// The target directory it is built in.
    pub target: PathBuf,
    /// The `.wasm` the last good build left.
    pub built: PathBuf,
}

impl Crate {
    pub fn manifest_path(&self) -> PathBuf {
        self.dir.join("Cargo.toml")
    }

    /// The manifest of the last good build or, before the first one, what
    /// its `Cargo.toml` declares.
    pub fn manifest(&self) -> Option<Manifest> {
        match std::fs::read(&self.built) {
            Ok(wasm) => Manifest::read(&wasm).ok()?,
            Err(_) => self.declared().ok(),
        }
    }

    /// What its `Cargo.toml` declares.
    pub fn declared(&self) -> Result<Manifest> {
        Manifest::from_crate(&self.dir).map_err(anyhow::Error::msg)
    }
}

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

    pub fn krate(&self) -> Crate {
        let build = build_dir(&self.project);
        Crate {
            dir: self.dir(),
            target: build.join(TARGET),
            built: build.join(format!("{}.wasm", self.name)),
        }
    }
}

/// A crate anywhere on disk, loaded into a project from its folder. It is
/// not pinned: it is built from what the folder holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loaded {
    /// The crate's name, which names its directories.
    pub id: String,
    /// The crate's directory, absolute. What the project lists it by.
    pub path: PathBuf,
}

impl Loaded {
    /// The crate in `dir`, named as its `Cargo.toml` names it.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = std::fs::canonicalize(dir)
            .with_context(|| format!("{} cannot be read", dir.display()))?;
        let manifest = Manifest::from_crate(&path).map_err(anyhow::Error::msg)?;
        Ok(Self {
            id: manifest.id,
            path,
        })
    }

    /// What the project lists it by: its folder.
    pub fn name(&self) -> Cow<'_, str> {
        self.path.to_string_lossy()
    }

    /// Load it into `project`, or write where it now is. An app applied under
    /// the same id from somewhere else is refused.
    pub fn save(&self, project: &Path) -> Result<()> {
        Record::Loaded(self.clone()).save(project)
    }

    pub fn krate(&self, project: &Path) -> Crate {
        let build = build_dir(project);
        Crate {
            dir: self.path.clone(),
            target: build.join(TARGET),
            built: build.join(LOADED).join(format!("{}.wasm", self.id)),
        }
    }
}

/// What a record under `.cydonia/apps/` holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Record {
    Installed(Installed),
    Loaded(Loaded),
}

impl Record {
    /// The records applied to `project`, by what each was applied from.
    fn list(project: &Path) -> Vec<Self> {
        let Ok(dirs) = std::fs::read_dir(applied_dir(project)) else {
            return Vec::new();
        };
        let mut records: Vec<Self> = dirs
            .flatten()
            .filter_map(|dir| std::fs::read_to_string(dir.path().join(RECORD)).ok())
            .filter_map(|text| toml::from_str(&text).ok())
            .collect();
        records.sort_by_key(|record| record.from());
        records
    }

    fn id(&self) -> &str {
        match self {
            Self::Installed(app) => &app.id,
            Self::Loaded(app) => &app.id,
        }
    }

    /// The reference or the folder it was applied from.
    fn from(&self) -> String {
        match self {
            Self::Installed(app) => app.reference.clone(),
            Self::Loaded(app) => app.name().into_owned(),
        }
    }

    /// Apply it to `project`, or write what changed about it there. An app
    /// applied under the same id from somewhere else is refused.
    fn save(&self, project: &Path) -> Result<()> {
        let path = applied_dir(project)
            .join(component(self.id())?)
            .join(RECORD);
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(held) = toml::from_str::<Self>(&text)
            && held.from() != self.from()
        {
            bail!("{} is already applied here from {}", self.id(), held.from());
        }
        crate::fs::save(&path, toml::to_string(self)?)?;
        Ok(())
    }
}

fn build_dir(project: &Path) -> PathBuf {
    fs::Project::new(project).cydonia().join(BUILD)
}

/// Watch a project's apps: its `apps/`, the folders loaded into it and its
/// records. `changed` hears the name of each app a change lands in, a crate's
/// `target/` left out, and an empty name where what is listed may have moved:
/// `apps/` appearing, or a record written. A project with no `apps/` is
/// watched shallowly at its root, unsettled. The folders watched are the ones
/// recorded when it is armed.
pub fn watch(project: &Path, changed: impl Fn(&str) + Send + Sync + 'static) -> Option<Watching> {
    let root = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let apps = root.join(DIR);
    let records = applied_dir(&root);
    let cydonia = fs::Project::new(&root).cydonia();
    let loaded: Vec<(String, PathBuf)> = Record::list(project)
        .into_iter()
        .filter_map(|record| match record {
            Record::Loaded(app) => Some((app.name().into_owned(), app.path)),
            Record::Installed(_) => None,
        })
        .collect();
    let crates = loaded.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        for path in &event.paths {
            if path == &apps || path.starts_with(&records) {
                changed("");
                continue;
            }
            if path.starts_with(&cydonia) || crate::fs::is_temporary(path) {
                continue;
            }
            if let Ok(rest) = path.strip_prefix(&apps) {
                let mut parts = rest.components();
                let Some(name) = parts.next().and_then(|name| name.as_os_str().to_str()) else {
                    continue;
                };
                if !parts.next().is_some_and(|part| part.as_os_str() == TARGET) {
                    changed(name);
                }
                continue;
            }
            for (name, dir) in &crates {
                if let Ok(rest) = path.strip_prefix(dir)
                    && !rest
                        .components()
                        .next()
                        .is_some_and(|part| part.as_os_str() == TARGET)
                {
                    changed(name);
                }
            }
        }
    })
    .ok()?;
    let _ = watcher.watch(&applied_dir(project), RecursiveMode::Recursive);
    for (_, dir) in &loaded {
        let _ = watcher.watch(dir, RecursiveMode::Recursive);
    }
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
        Record::list(project)
            .into_iter()
            .filter_map(|record| match record {
                Record::Installed(app) => Some(app),
                Record::Loaded(_) => None,
            })
            .collect()
    }

    /// Apply it to `project`, or write what changed about it there. An app
    /// applied under the same id from somewhere else is refused.
    pub fn save(&self, project: &Path) -> Result<()> {
        Record::Installed(self.clone()).save(project)
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
