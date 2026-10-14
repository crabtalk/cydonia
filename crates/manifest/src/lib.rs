//! What a Cydonia app says about itself: the manifest in a custom section of
//! its `.wasm`, which Cydonia reads without running it, and the
//! `[package.metadata.cydonia]` table in its `Cargo.toml` that `cydonia-guest`'s
//! `app!` writes it from.

use serde::{Deserialize, Serialize};

/// The version of the node and event vocabulary `cydonia-guest` writes. Nodes
/// and fields are only ever added to it.
pub const CONTRACT: u32 = 1;

/// The custom section's name.
pub const SECTION: &str = "cydonia.manifest";

/// The groups of calls an app may declare.
pub mod calls {
    /// [`crate::tables`].
    pub const TABLES: &str = "tables";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The app's crate name.
    pub id: String,
    /// What Cydonia lists the app as.
    pub name: String,
    /// The app's crate version.
    pub version: String,
    /// The [`CONTRACT`] the app was built against.
    pub contract: u32,
    /// The groups of calls it makes, from [`calls`].
    #[serde(default)]
    pub calls: Vec<String>,
    /// The hosts it fetches from.
    #[serde(default)]
    pub hosts: Vec<String>,
}

impl Manifest {
    /// The manifest in `wasm`'s custom section. `None` for a module that has
    /// none.
    pub fn read(wasm: &[u8]) -> Result<Option<Self>, String> {
        let Some(mut rest) = wasm.strip_prefix(b"\0asm") else {
            return Err("not a wasm module".into());
        };
        rest = rest.get(4..).ok_or("a wasm module cut short")?;
        while let Some((&id, after)) = rest.split_first() {
            let (size, after) = leb(after)?;
            let body = after.get(..size).ok_or("a wasm section cut short")?;
            rest = &after[size..];
            if id != 0 {
                continue;
            }
            let (len, body) = leb(body)?;
            let name = body.get(..len).ok_or("a custom section's name cut short")?;
            if name == SECTION.as_bytes() {
                return serde_json::from_slice(&body[len..])
                    .map(Some)
                    .map_err(|error| format!("an unreadable manifest: {error}"));
            }
        }
        Ok(None)
    }

    /// Refuses an app built against a newer [`CONTRACT`] than this
    /// one, naming the version it needs.
    pub fn supported(&self) -> Result<(), String> {
        match self.contract > CONTRACT {
            true => Err(format!(
                "{} needs view contract {}, and this Cydonia has {}",
                self.name, self.contract, CONTRACT
            )),
            false => Ok(()),
        }
    }

    /// Whether `url` is an `http` or `https` URL on one of the hosts the app
    /// names. A URL carrying a user or a password is not.
    pub fn reaches(&self, url: &str) -> bool {
        let Some(rest) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
        else {
            return false;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return false;
        }
        let host = match authority.rsplit_once(':') {
            Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
            _ => authority,
        };
        self.hosts
            .iter()
            .any(|named| named.eq_ignore_ascii_case(host))
    }

    /// Whether the app declared the group of calls `group`.
    pub fn calls(&self, group: &str) -> bool {
        self.calls.iter().any(|call| call == group)
    }
}

/// An unsigned LEB128 length and what follows it.
fn leb(bytes: &[u8]) -> Result<(usize, &[u8]), String> {
    let mut value = 0usize;
    for (ix, byte) in bytes.iter().enumerate().take(5) {
        value |= ((byte & 0x7f) as usize) << (7 * ix);
        if byte & 0x80 == 0 {
            return Ok((value, &bytes[ix + 1..]));
        }
    }
    Err("a malformed wasm length".into())
}

/// What an app declares in `[package.metadata.cydonia]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declared {
    /// What Cydonia lists the app as. The crate's name where it is left out.
    #[serde(default)]
    pub name: Option<String>,
    /// The groups of calls it makes, from [`calls`].
    #[serde(default)]
    pub calls: Vec<String>,
    /// The hosts it fetches from.
    #[serde(default)]
    pub hosts: Vec<String>,
}

impl Declared {
    /// The manifest of the crate `id` at `version`, built against this
    /// [`CONTRACT`].
    pub fn manifest(self, id: String, version: String) -> Manifest {
        Manifest {
            name: self.name.unwrap_or_else(|| id.clone()),
            id,
            version,
            contract: CONTRACT,
            calls: self.calls,
            hosts: self.hosts,
        }
    }
}

#[cfg(feature = "cargo")]
mod cargo {
    use super::{Declared, Manifest};
    use serde::Deserialize;
    use std::path::Path;

    #[derive(Deserialize)]
    struct File {
        package: Package,
    }

    #[derive(Deserialize)]
    struct Package {
        name: String,
        /// A table where the version is inherited from a workspace.
        #[serde(default)]
        version: Option<toml::Value>,
        #[serde(default)]
        metadata: Metadata,
    }

    #[derive(Default, Deserialize)]
    struct Metadata {
        #[serde(default)]
        cydonia: Declared,
    }

    fn package(toml: &str) -> Result<Package, String> {
        toml::from_str::<File>(toml)
            .map(|file| file.package)
            .map_err(|error| format!("an unreadable Cargo.toml: {error}"))
    }

    impl Declared {
        /// The `[package.metadata.cydonia]` table of the `Cargo.toml` `toml`,
        /// or an empty one where it has none.
        pub fn from_cargo(toml: &str) -> Result<Self, String> {
            package(toml).map(|package| package.metadata.cydonia)
        }
    }

    impl Manifest {
        /// The manifest the crate whose `Cargo.toml` is `toml` declares. Its
        /// version is empty where the crate inherits it from a workspace.
        pub fn from_cargo(toml: &str) -> Result<Self, String> {
            let package = package(toml)?;
            let version = package
                .version
                .as_ref()
                .and_then(toml::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(package.metadata.cydonia.manifest(package.name, version))
        }

        /// The manifest the crate in `dir` declares in its `Cargo.toml`.
        pub fn from_crate(dir: &Path) -> Result<Self, String> {
            let path = dir.join("Cargo.toml");
            let toml = std::fs::read_to_string(&path)
                .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
            Self::from_cargo(&toml)
        }
    }
}
