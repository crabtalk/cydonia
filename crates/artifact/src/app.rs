//! The apps on this machine, each an [`Origin`]: a crate loaded from a folder,
//! which Cydonia builds, or an app installed from a registry. An app belongs
//! to no project. Each keeps its record and its own things under the data
//! directory's `app/<id>/`.

use crate::project::{Watching, fs::component};
use anyhow::{Context as _, Result, bail};
use manifest::Manifest;
use notify::{RecursiveMode, Watcher as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Under the data directory: every app's directory.
const DIR: &str = "app";
/// Under the data directory: the target directory loaded apps are built in.
const BUILD: &str = "build";
/// An app's record, in its directory.
const RECORD: &str = "app.toml";
/// In a loaded app's directory: its last good build.
const BUILT: &str = "built.wasm";
/// The directory cargo builds into, inside a crate.
const TARGET: &str = "target";

/// Where an app comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A crate loaded from a folder.
    Loaded(Loaded),
    /// An app installed from a registry.
    Installed(Installed),
}

impl Origin {
    /// Every app on this machine, by id.
    pub fn list() -> Vec<Self> {
        let Ok(dir) = apps_dir() else {
            return Vec::new();
        };
        let Ok(dirs) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut apps: Vec<Self> = dirs
            .flatten()
            .filter_map(|dir| std::fs::read_to_string(dir.path().join(RECORD)).ok())
            .filter_map(|text| toml::from_str(&text).ok())
            .map(|record| match record {
                Record::Installed(app) => Self::Installed(app),
                Record::Loaded(app) => Self::Loaded(app),
            })
            .collect();
        apps.sort_by(|a, b| a.id().cmp(b.id()));
        apps
    }

    /// Its manifest's id, which names its directory.
    pub fn id(&self) -> &str {
        match self {
            Self::Loaded(app) => &app.id,
            Self::Installed(app) => &app.id,
        }
    }

    /// Whether Cydonia builds it from a crate on this machine.
    pub fn local(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    /// Whether the app may run without asking first. An installed app asks
    /// for each digest it runs.
    pub fn allowed(&self) -> bool {
        match self {
            Self::Loaded(_) => true,
            Self::Installed(app) => app.allowed(),
        }
    }

    /// Its manifest: an installed app's, or a crate's from its last good
    /// build or, before the first one, its `Cargo.toml`.
    pub fn manifest(&self) -> Option<Manifest> {
        match self {
            Self::Installed(app) => app.manifest(),
            Self::Loaded(app) => app.krate().ok()?.manifest(),
        }
    }

    /// Its directory: its record and its own things.
    pub fn dir(&self) -> Result<PathBuf> {
        app_dir(self.id())
    }

    /// Forget it: its record and its own things go. A loaded crate's folder
    /// stays.
    pub fn remove(&self) -> Result<()> {
        let dir = self.dir()?;
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("{} cannot be removed", dir.display()))
    }

    /// What the app is listed as: its manifest's name, or its id.
    pub fn title(&self) -> String {
        self.manifest()
            .map(|manifest| manifest.name)
            .unwrap_or_else(|| self.id().to_owned())
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

/// A crate anywhere on disk, loaded from its folder. It is not pinned: it is
/// built from what the folder holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loaded {
    /// The crate's name, which names its directory.
    pub id: String,
    /// The crate's directory, absolute.
    pub path: PathBuf,
}

impl Loaded {
    /// The crate in `dir`, named as its `Cargo.toml` names it. A crate that
    /// does not depend on `cydonia-guest` is refused.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = std::fs::canonicalize(dir)
            .with_context(|| format!("{} cannot be read", dir.display()))?;
        let manifest = Manifest::from_crate(&path).map_err(anyhow::Error::msg)?;
        Ok(Self {
            id: manifest.id,
            path,
        })
    }

    /// Record it, or write where it now is. An app recorded under the same id
    /// from somewhere else is refused.
    pub fn save(&self) -> Result<()> {
        Record::Loaded(self.clone()).save()
    }

    pub fn krate(&self) -> Result<Crate> {
        Ok(Crate {
            dir: self.path.clone(),
            target: crate::backup::data_dir()?.join(BUILD),
            built: app_dir(&self.id)?.join(BUILT),
        })
    }
}

/// An app installed from a registry: where it was pulled from, and the digest
/// its `.wasm` is pinned to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    /// Its manifest's id, which names its directory.
    pub id: String,
    /// The repository it was pulled from, without a tag:
    /// `ghcr.io/crabtalk/cydonia/yc`.
    pub reference: String,
    /// The tag it was pulled by.
    pub tag: String,
    /// The `.wasm`'s digest, `sha256:<hex>`.
    pub digest: String,
    /// The digest the person has let run, after its permission prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<String>,
}

impl Installed {
    /// Record it, or write what changed about it. An app recorded under the
    /// same id from somewhere else is refused.
    pub fn save(&self) -> Result<()> {
        Record::Installed(self.clone()).save()
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
        Ok(app_dir(id)?.join(format!("{hex}.wasm")))
    }

    /// Its manifest, read from its kept `.wasm`.
    pub fn manifest(&self) -> Option<Manifest> {
        Manifest::read(&std::fs::read(self.wasm().ok()?).ok()?).ok()?
    }
}

/// What an app's record holds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Record {
    Installed(Installed),
    Loaded(Loaded),
}

impl Record {
    fn id(&self) -> &str {
        match self {
            Self::Installed(app) => &app.id,
            Self::Loaded(app) => &app.id,
        }
    }

    /// The reference or the folder it came from.
    fn from(&self) -> String {
        match self {
            Self::Installed(app) => app.reference.clone(),
            Self::Loaded(app) => app.path.to_string_lossy().into_owned(),
        }
    }

    /// Write it, or what changed about it. An app recorded under the same id
    /// from somewhere else is refused.
    fn save(&self) -> Result<()> {
        let path = app_dir(self.id())?.join(RECORD);
        if let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(held) = toml::from_str::<Self>(&text)
            && held.from() != self.from()
        {
            bail!("{} is already here from {}", self.id(), held.from());
        }
        crate::fs::save(&path, toml::to_string(self)?)?;
        Ok(())
    }
}

/// Where an app keeps its own things: an installed app's pinned `.wasm`s, a
/// loaded app's last build, and its tables.
pub trait Home {
    fn home(&self) -> Result<PathBuf>;
}

impl Home for Manifest {
    fn home(&self) -> Result<PathBuf> {
        app_dir(&self.id)
    }
}

/// The data directory's `app/`.
fn apps_dir() -> Result<PathBuf> {
    Ok(crate::backup::data_dir()?.join(DIR))
}

/// The data directory's `app/<id>/`.
fn app_dir(id: &str) -> Result<PathBuf> {
    Ok(apps_dir()?.join(component(id)?))
}

/// Watch the apps: their records and the folders loaded apps are built from.
/// `changed` hears the id of each loaded app a change lands in, a crate's
/// `target/` left out, and an empty id where a record was written or removed.
/// The folders watched are the ones recorded when it is armed.
pub fn watch(changed: impl Fn(&str) + Send + Sync + 'static) -> Option<Watching> {
    let apps = apps_dir().ok()?;
    std::fs::create_dir_all(&apps).ok()?;
    let apps = std::fs::canonicalize(&apps).unwrap_or(apps);
    let loaded: Vec<(String, PathBuf)> = Origin::list()
        .into_iter()
        .filter_map(|origin| match origin {
            Origin::Loaded(app) => Some((app.id, app.path)),
            Origin::Installed(_) => None,
        })
        .collect();
    let crates = loaded.clone();
    let records = apps.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        for path in &event.paths {
            if crate::fs::is_temporary(path) {
                continue;
            }
            if let Ok(rest) = path.strip_prefix(&records) {
                let depth = rest.components().count();
                if depth == 1 || (depth == 2 && path.file_name().is_some_and(|name| name == RECORD))
                {
                    changed("");
                }
                continue;
            }
            for (id, dir) in &crates {
                if let Ok(rest) = path.strip_prefix(dir)
                    && !rest
                        .components()
                        .next()
                        .is_some_and(|part| part.as_os_str() == TARGET)
                {
                    changed(id);
                }
            }
        }
    })
    .ok()?;
    for (_, dir) in &loaded {
        let _ = watcher.watch(dir, RecursiveMode::Recursive);
    }
    watcher
        .watch(&apps, RecursiveMode::Recursive)
        .ok()
        .map(|()| Watching::new(true, watcher))
}
