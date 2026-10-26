//! Agent identities: a named agent with a workspace, the program it runs on
//! and that program's choices. Kept in `~/.config/cydonia/agents/`, one
//! directory per identity named by its handle, holding `agent.toml`.
//!
//! Without the `desktop` feature there is no config directory, and identities
//! are held in memory for as long as the process runs.

use crate::model::session_preferences::Choices;
#[cfg(feature = "desktop")]
use crate::model::settings;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "desktop")]
const FILE: &str = "agent.toml";
/// The handle of an identity whose name leaves nothing to make one from.
const FALLBACK_HANDLE: &str = "agent";

/// Where this build keeps identities.
#[cfg(feature = "desktop")]
pub type Shelf = Disk;
#[cfg(not(feature = "desktop"))]
pub type Shelf = Memory;

/// A named agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    /// A UUIDv7, minted when the identity is made and never reminted. Session
    /// records and links name the identity by it.
    pub id: String,
    /// The name of its directory, and not written inside it. Lowercase
    /// letters, digits and `-`, unique on this machine.
    #[serde(skip)]
    pub handle: String,
    pub name: String,
    /// The project directory its sessions run in.
    pub workspace: PathBuf,
    /// The program it runs on, by its registry id in `settings.toml`'s
    /// `[[agents]]`.
    pub acp: String,
    pub avatar: Avatar,
    #[serde(flatten)]
    pub choices: Choices,
}

/// The seed an identity's blob avatar is drawn from, by bezel's
/// `agent::Face::from(u64)`. Written as 16 hex digits: a TOML integer stops at
/// `i64::MAX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Avatar(pub u64);

impl Avatar {
    /// A seed off the random bits of a fresh UUIDv7.
    pub fn random() -> Self {
        Self::from(uuid::Uuid::now_v7())
    }
}

/// The low half of the id, which in a UUIDv7 is its random bits.
impl From<uuid::Uuid> for Avatar {
    fn from(id: uuid::Uuid) -> Self {
        Self(id.as_u64_pair().1)
    }
}

impl From<Avatar> for String {
    fn from(avatar: Avatar) -> Self {
        format!("{:016x}", avatar.0)
    }
}

impl TryFrom<String> for Avatar {
    type Error = std::num::ParseIntError;

    fn try_from(hex: String) -> Result<Self, Self::Error> {
        u64::from_str_radix(&hex, 16).map(Self)
    }
}

impl Identity {
    /// The handle `name` makes: lowercased, every run of anything but a
    /// letter or a digit one `-`, none at either end.
    pub fn handle_for(name: &str) -> String {
        let mut handle = String::new();
        for c in name.chars().flat_map(char::to_lowercase) {
            if c.is_ascii_alphanumeric() {
                handle.push(c);
            } else if !handle.is_empty() && !handle.ends_with('-') {
                handle.push('-');
            }
        }
        let handle = handle.trim_end_matches('-');
        match handle.is_empty() {
            true => FALLBACK_HANDLE.to_owned(),
            false => handle.to_owned(),
        }
    }
}

/// What keeps identities.
pub trait Store {
    /// Every identity held, in no order.
    fn all() -> Vec<Identity>;

    /// Write one back under its handle.
    fn save(identity: &Identity);

    /// Remove the identity under `handle`, its directory with it.
    fn remove(handle: &str);

    /// Move the identity under `from` to `to`. `false` where `to` is taken or
    /// nothing moved.
    fn rename(from: &str, to: &str) -> bool;

    /// Whether an identity is held under `handle`.
    fn holds(handle: &str) -> bool;

    /// Ready to take a new identity; `false` where nothing can be written.
    fn prepare() -> bool;

    /// Mint one, its handle made from `name` and suffixed clear of those
    /// already here, its avatar seeded from its id.
    fn create(name: &str, workspace: PathBuf, acp: &str, choices: Choices) -> Option<Identity> {
        if !Self::prepare() {
            return None;
        }
        let handle = artifact::id::suffixed(&Identity::handle_for(name), Self::holds);
        let id = uuid::Uuid::now_v7();
        let identity = Identity {
            id: id.to_string(),
            handle,
            name: name.to_owned(),
            workspace,
            acp: acp.to_owned(),
            avatar: Avatar::from(id),
            choices,
        };
        Self::save(&identity);
        Some(identity)
    }
}

/// `~/.config/cydonia/agents/`, one directory per identity.
#[cfg(feature = "desktop")]
pub struct Disk;

#[cfg(feature = "desktop")]
impl Disk {
    fn dir() -> Option<PathBuf> {
        settings::dir().ok().map(|dir| dir.join("agents"))
    }

    fn home(handle: &str) -> Option<PathBuf> {
        Some(Self::dir()?.join(handle))
    }

    fn read(handle: &str) -> Option<Identity> {
        let body = std::fs::read_to_string(Self::home(handle)?.join(FILE)).ok()?;
        let mut identity: Identity = toml::from_str(&body).ok()?;
        identity.handle = handle.to_owned();
        Some(identity)
    }
}

#[cfg(feature = "desktop")]
impl Store for Disk {
    fn all() -> Vec<Identity> {
        let Some(dir) = Self::dir() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| Self::read(entry.file_name().to_str()?))
            .collect()
    }

    fn save(identity: &Identity) {
        let Some(home) = Self::home(&identity.handle) else {
            return;
        };
        if std::fs::create_dir_all(&home).is_ok() {
            let _ = settings::write_toml(&home.join(FILE), identity);
        }
    }

    fn remove(handle: &str) {
        if let Some(home) = Self::home(handle) {
            let _ = std::fs::remove_dir_all(home);
        }
    }

    fn rename(from: &str, to: &str) -> bool {
        let (Some(from), Some(to)) = (Self::home(from), Self::home(to)) else {
            return false;
        };
        !to.exists() && std::fs::rename(from, to).is_ok()
    }

    fn holds(handle: &str) -> bool {
        Self::home(handle).is_some_and(|home| home.exists())
    }

    fn prepare() -> bool {
        Self::dir().is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
    }
}

/// Held for as long as the process runs.
#[cfg(not(feature = "desktop"))]
pub struct Memory;

#[cfg(not(feature = "desktop"))]
impl Memory {
    fn held() -> std::sync::MutexGuard<'static, std::collections::BTreeMap<String, Identity>> {
        static HELD: std::sync::Mutex<std::collections::BTreeMap<String, Identity>> =
            std::sync::Mutex::new(std::collections::BTreeMap::new());
        HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(not(feature = "desktop"))]
impl Store for Memory {
    fn all() -> Vec<Identity> {
        Self::held().values().cloned().collect()
    }

    fn save(identity: &Identity) {
        Self::held().insert(identity.handle.clone(), identity.clone());
    }

    fn remove(handle: &str) {
        Self::held().remove(handle);
    }

    fn rename(from: &str, to: &str) -> bool {
        let mut held = Self::held();
        if held.contains_key(to) {
            return false;
        }
        let Some(mut identity) = held.remove(from) else {
            return false;
        };
        identity.handle = to.to_owned();
        held.insert(to.to_owned(), identity);
        true
    }

    fn holds(handle: &str) -> bool {
        Self::held().contains_key(handle)
    }

    fn prepare() -> bool {
        true
    }
}
