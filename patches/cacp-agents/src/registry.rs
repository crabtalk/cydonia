//! The protocol's catalog of ACP agents, pinned to exact versions.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";

/// How long a cached catalog is served before refetching.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// The catalog of known ACP agents.
#[derive(Debug)]
pub struct Registry {
    pub version: String,
    pub agents: Vec<Agent>,
}

/// One agent in the catalog, at the version the registry pins.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repository: Option<String>,
    /// The project's own page, for an agent that publishes no repository — a
    /// proprietary one has somewhere to point even with no source to show.
    pub website: Option<String>,
    pub icon: Option<String>,
    pub distribution: Distribution,
}

/// How an agent is obtained. Kinds that cannot be installed yet are kept rather
/// than dropped, so callers can say why an agent is unavailable.
#[derive(Debug, Clone)]
pub enum Distribution {
    /// An npm package run over stdio.
    Npm { package: String, args: Vec<String> },
    /// A release archive for *this* machine, already narrowed from the registry's
    /// per-platform table — a build for someone else's platform is no more
    /// installable here than a kind we can't read at all.
    Binary(Binary),
    /// Other package managers, or a binary with no build for this platform.
    Unsupported { kind: String },
}

/// One platform's release archive, and what to run once it is unpacked.
#[derive(Debug, Clone)]
pub struct Binary {
    pub archive: String,
    /// The executable, relative to the root the archive unpacks into.
    pub cmd: String,
    /// Absent for roughly half the catalog, which is the publisher's choice and
    /// not a signal about the download — see `install`.
    pub sha256: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl Agent {
    pub fn installable(&self) -> bool {
        matches!(
            self.distribution,
            Distribution::Npm { .. } | Distribution::Binary(_)
        )
    }
}

/// How the registry names this machine — `darwin-aarch64`, `linux-x86_64`. Rust
/// calls Apple's platform `macos` and the registry calls it `darwin`; the
/// architectures already agree.
fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

#[derive(Deserialize)]
struct WireRegistry {
    version: String,
    #[serde(default)]
    agents: Vec<WireAgent>,
}

#[derive(Deserialize)]
struct WireAgent {
    id: String,
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    distribution: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct WireNpx {
    package: String,
    #[serde(default)]
    args: Vec<String>,
}

/// One entry of the `binary` table, keyed by [`platform_key`].
#[derive(Deserialize)]
struct WireBinary {
    archive: String,
    cmd: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

/// Pick the way in. npm first wherever both are offered: it is the smaller
/// download and the one path that needs no per-platform build.
fn read_distribution(table: &BTreeMap<String, serde_json::Value>) -> Distribution {
    // `npx` is the wire name; we install the package rather than resolving it
    // per launch, hence `Npm` on our side.
    if let Some(value) = table.get("npx") {
        return match serde_json::from_value::<WireNpx>(value.clone()) {
            Ok(npx) => Distribution::Npm {
                package: npx.package,
                args: npx.args,
            },
            Err(_) => Distribution::Unsupported {
                kind: "npx".to_owned(),
            },
        };
    }
    if let Some(value) = table.get("binary") {
        let builds: BTreeMap<String, WireBinary> =
            serde_json::from_value(value.clone()).unwrap_or_default();
        if let Some(build) = builds
            .into_iter()
            .find_map(|(plat, b)| (plat == platform_key()).then_some(b))
        {
            return Distribution::Binary(Binary {
                archive: build.archive,
                cmd: build.cmd,
                sha256: build.sha256,
                args: build.args,
                env: build.env,
            });
        }
        // Published, but not for this machine.
        return Distribution::Unsupported {
            kind: "binary".to_owned(),
        };
    }
    Distribution::Unsupported {
        kind: table
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned()),
    }
}

/// Read a catalog document, for a caller that fetched it itself.
pub fn parse(body: &str) -> Result<Registry> {
    let wire: WireRegistry = serde_json::from_str(body).context("malformed registry json")?;
    let agents = wire
        .agents
        .into_iter()
        .map(|agent| {
            let distribution = read_distribution(&agent.distribution);
            Agent {
                id: agent.id,
                name: agent.name,
                version: agent.version,
                description: agent.description,
                repository: agent.repository,
                website: agent.website,
                icon: agent.icon,
                distribution,
            }
        })
        .collect();
    Ok(Registry {
        version: wire.version,
        agents,
    })
}

fn cache_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("registry.json")
}

/// Fetch the catalog and refresh the cache.
pub fn fetch(cache_dir: &Path) -> Result<Registry> {
    let body = ureq::get(REGISTRY_URL)
        .call()
        .context("fetching the ACP registry")?
        .body_mut()
        .read_to_string()
        .context("reading the ACP registry")?;
    let registry = parse(&body)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(cache_file(cache_dir), &body);
    Ok(registry)
}

/// The cached catalog, however old.
pub fn cached(cache_dir: &Path) -> Option<Registry> {
    let body = std::fs::read_to_string(cache_file(cache_dir)).ok()?;
    parse(&body).ok()
}

/// The catalog for normal use: a fresh cache is served as-is, otherwise the
/// network is tried and a stale cache covers failure. `None` only when there is
/// neither cache nor connectivity — the caller's own configured agents still
/// work.
pub fn catalog(cache_dir: &Path) -> Option<Registry> {
    let fresh = std::fs::metadata(cache_file(cache_dir))
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or(CACHE_TTL) < CACHE_TTL);
    if fresh && let Some(registry) = cached(cache_dir) {
        return Some(registry);
    }
    fetch(cache_dir).ok().or_else(|| cached(cache_dir))
}
