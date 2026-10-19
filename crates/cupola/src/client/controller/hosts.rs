//! The saved hosts, in a TOML file: a `next` id and one `[[host]]` table per
//! host, with its `id`, `name`, `destination` and optional `port`.

use std::{collections::HashSet, fmt, fs, io, path::PathBuf};

use anyhow::{Context as _, bail};
use serde::{Deserialize, Serialize};

use crate::{client::ssh::Host, fs::Replacement};

/// A saved host's id. Never given to another host of the same file, removed
/// or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostId(u64);

/// One saved host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    id: HostId,
    #[serde(default)]
    name: String,
    #[serde(flatten)]
    host: Host,
}

/// The saved hosts as last written to their file.
pub(crate) struct Hosts {
    path: PathBuf,
    file: File,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct File {
    /// The id the next added host gets.
    #[serde(default)]
    next: u64,
    #[serde(default, rename = "host")]
    hosts: Vec<Saved>,
}

/// A host's name: trimmed, and not empty.
struct Name(String);

impl fmt::Display for HostId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Saved {
    pub fn id(&self) -> HostId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn host(&self) -> &Host {
        &self.host
    }
}

impl Hosts {
    /// Reads the file at `path`; none there reads as no hosts. A host without
    /// a name reads as named by its endpoint, a host whose id came earlier in
    /// the file is skipped, and `next` reads as above every id.
    pub(crate) fn load(path: PathBuf) -> anyhow::Result<Self> {
        let mut file: File = match fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("read {}", path.display()))?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => File::default(),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        let mut seen = HashSet::new();
        file.hosts.retain(|saved| seen.insert(saved.id));
        for saved in &mut file.hosts {
            match Name::new(&saved.name) {
                Some(name) => saved.name = name.0,
                None => saved.name = saved.host.to_string(),
            }
            file.next = file.next.max(saved.id.0 + 1);
        }
        Ok(Self { path, file })
    }

    pub(crate) fn list(&self) -> &[Saved] {
        &self.file.hosts
    }

    pub(crate) fn get(&self, id: HostId) -> Option<&Saved> {
        self.file.hosts.iter().find(|saved| saved.id == id)
    }

    pub(crate) fn add(&mut self, name: &str, host: Host) -> anyhow::Result<HostId> {
        let name = Name::parse(name)?;
        let mut file = self.file.clone();
        let id = HostId(file.next);
        file.next += 1;
        file.hosts.push(Saved {
            id,
            name: name.0,
            host,
        });
        self.write(file)?;
        Ok(id)
    }

    pub(crate) fn rename(&mut self, id: HostId, name: &str) -> anyhow::Result<()> {
        let name = Name::parse(name)?;
        let mut file = self.file.clone();
        let saved = file
            .hosts
            .iter_mut()
            .find(|saved| saved.id == id)
            .with_context(|| format!("no host {id}"))?;
        saved.name = name.0;
        self.write(file)
    }

    pub(crate) fn remove(&mut self, id: HostId) -> anyhow::Result<()> {
        let mut file = self.file.clone();
        let before = file.hosts.len();
        file.hosts.retain(|saved| saved.id != id);
        if file.hosts.len() == before {
            bail!("no host {id}");
        }
        self.write(file)
    }

    /// Replaces the file with `file` through a temporary file beside it, and
    /// keeps `file` once it is in place.
    fn write(&mut self, file: File) -> anyhow::Result<()> {
        let text = toml::to_string(&file).context("encode the hosts")?;
        let written = (|| {
            if let Some(dir) = self.path.parent() {
                fs::create_dir_all(dir)?;
            }
            Replacement::beside(&self.path, text.as_bytes())?.land(&self.path)
        })();
        written.with_context(|| format!("write {}", self.path.display()))?;
        self.file = file;
        Ok(())
    }
}

impl Name {
    fn new(name: &str) -> Option<Self> {
        let name = name.trim();
        (!name.is_empty()).then(|| Self(name.to_owned()))
    }

    fn parse(name: &str) -> anyhow::Result<Self> {
        Self::new(name).context("a host needs a name")
    }
}
