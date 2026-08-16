//! The official ACP agent registry, and a managed installer for it.
//!
//! The registry is the protocol's own catalog of ACP agents, pinned to
//! exact versions — so an agent's build never changes underfoot the way
//! `npx <pkg>@latest` does. Agents install once into a data directory
//! and run straight from there, keeping package managers out of the
//! chat path entirely.
//!
//! Deliberately standalone: no dependency on the rest of cydonia, so
//! any ACP client can use it.

pub mod mcp;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
    pub distribution: Distribution,
}

/// How an agent is obtained. Kinds cydonia can't install yet are kept
/// (rather than dropped) so callers can say why an agent is unavailable.
#[derive(Debug, Clone)]
pub enum Distribution {
    /// An npm package run over stdio.
    Npm { package: String, args: Vec<String> },
    /// Platform archives or other package managers — not installable yet.
    Unsupported { kind: String },
}

impl Agent {
    pub fn installable(&self) -> bool {
        matches!(self.distribution, Distribution::Npm { .. })
    }
}

/// A local installation: the command to spawn and its arguments.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

// ── Catalog ──────────────────────────────────────────────────────

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
    distribution: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct WireNpx {
    package: String,
    #[serde(default)]
    args: Vec<String>,
}

fn parse(body: &str) -> Result<Registry> {
    let wire: WireRegistry = serde_json::from_str(body).context("malformed registry json")?;
    let agents = wire
        .agents
        .into_iter()
        .map(|agent| {
            // `npx` is the wire name; we install the package rather than
            // resolving it per launch, hence `Npm` on our side.
            let distribution = match agent.distribution.get("npx") {
                Some(value) => match serde_json::from_value::<WireNpx>(value.clone()) {
                    Ok(npx) => Distribution::Npm {
                        package: npx.package,
                        args: npx.args,
                    },
                    Err(_) => Distribution::Unsupported {
                        kind: "npx".to_owned(),
                    },
                },
                None => Distribution::Unsupported {
                    kind: agent
                        .distribution
                        .keys()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_owned()),
                },
            };
            Agent {
                id: agent.id,
                name: agent.name,
                version: agent.version,
                description: agent.description,
                repository: agent.repository,
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

/// The catalog for normal use: a fresh cache is served as-is, otherwise
/// the network is tried and a stale cache covers failure. Returns
/// `None` only when there is neither cache nor connectivity — the
/// caller's own configured agents still work.
pub fn catalog(cache_dir: &Path) -> Option<Registry> {
    let fresh = std::fs::metadata(cache_file(cache_dir))
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or(CACHE_TTL) < CACHE_TTL);
    if fresh && let Some(registry) = cached(cache_dir) {
        return Some(registry);
    }
    fetch(cache_dir).ok().or_else(|| cached(cache_dir))
}

// ── Installation ─────────────────────────────────────────────────

fn agent_dir(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("agents").join(id)
}

fn record_file(data_dir: &Path, id: &str) -> PathBuf {
    agent_dir(data_dir, id).join("cydonia-install.json")
}

/// The recorded installation for `id`, if the agent is installed and
/// its command still exists.
pub fn installed(data_dir: &Path, id: &str) -> Option<Installed> {
    let body = std::fs::read_to_string(record_file(data_dir, id)).ok()?;
    let record: Installed = serde_json::from_str(&body).ok()?;
    Path::new(&record.command).exists().then_some(record)
}

/// The package name in a spec like `@scope/name@1.2.3` (the version
/// separator is the last `@` that isn't the scope's leading one).
fn package_name(spec: &str) -> &str {
    match spec.rfind('@') {
        Some(ix) if ix > 0 => &spec[..ix],
        _ => spec,
    }
}

/// Install `agent` under `data_dir`, streaming installer output to
/// `on_line`. Replaces any previous install of the same agent.
pub fn install(data_dir: &Path, agent: &Agent, on_line: impl FnMut(&str)) -> Result<Installed> {
    let Distribution::Npm { package, args } = &agent.distribution else {
        bail!("{} can't be installed by cydonia yet", agent.name);
    };
    let dir = agent_dir(data_dir, &agent.id);
    let command = install_npm(&dir, package, on_line)?;

    let record = Installed {
        id: agent.id.clone(),
        version: agent.version.clone(),
        command,
        args: args.clone(),
    };
    std::fs::write(
        record_file(data_dir, &agent.id),
        serde_json::to_string_pretty(&record)?,
    )
    .context("recording the installation")?;
    Ok(record)
}

/// Install one npm package into `dir` (replacing whatever was there),
/// streaming installer output to `on_line`. Returns the executable path.
pub(crate) fn install_npm(
    dir: &Path,
    package: &str,
    mut on_line: impl FnMut(&str),
) -> Result<String> {
    if which("npm").is_none() {
        bail!("npm was not found on PATH — install Node.js to add agents");
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    on_line(&format!("npm install {package}"));
    let mut child = Command::new("npm")
        .arg("install")
        .arg("--prefix")
        .arg(dir)
        .args(["--no-fund", "--no-audit", "--loglevel", "http"])
        .arg(package)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running npm")?;

    // npm splits progress across both streams; merge them so the
    // caller sees output in the order it arrives.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    for line in rx {
        let line = line.trim_end();
        if !line.is_empty() {
            on_line(line);
        }
    }

    let status = child.wait().context("waiting for npm")?;
    if !status.success() {
        bail!("npm install failed ({status})");
    }
    let command = binary_path(dir, package)?;
    on_line("installed");
    Ok(command)
}

/// The executable npm linked for `package`, read from the installed
/// package's own `bin` field.
fn binary_path(dir: &Path, package: &str) -> Result<String> {
    let name = package_name(package);
    let manifest = dir.join("node_modules").join(name).join("package.json");
    let body = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let manifest: serde_json::Value = serde_json::from_str(&body)?;

    let bin_name = match &manifest["bin"] {
        serde_json::Value::String(_) => name.rsplit('/').next().unwrap_or(name).to_owned(),
        serde_json::Value::Object(map) => map
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow!("{name} declares no executable"))?,
        _ => bail!("{name} declares no executable"),
    };

    let bin = dir.join("node_modules").join(".bin").join(&bin_name);
    if !bin.exists() {
        bail!("{} is missing after install", bin.display());
    }
    Ok(bin.display().to_string())
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_keep_their_scope() {
        assert_eq!(
            package_name("@agentclientprotocol/claude-agent-acp@0.68.0"),
            "@agentclientprotocol/claude-agent-acp"
        );
        assert_eq!(package_name("@scope/name"), "@scope/name");
        assert_eq!(package_name("plain@1.2.3"), "plain");
        assert_eq!(package_name("plain"), "plain");
    }

    #[test]
    fn parses_the_published_shape() {
        let registry = parse(
            r#"{"version":"1.0.0","agents":[
                {"id":"claude-acp","name":"Claude Agent","version":"0.68.0",
                 "distribution":{"npx":{"package":"@agentclientprotocol/claude-agent-acp@0.68.0"}}},
                {"id":"gemini","name":"Gemini CLI","version":"0.55.1",
                 "distribution":{"npx":{"package":"@google/gemini-cli@0.55.1","args":["--acp"]}}},
                {"id":"opencode","name":"OpenCode","version":"1.0.0",
                 "distribution":{"binary":{"darwin-aarch64":{"archive":"https://example.invalid/x.zip"}}}}
            ]}"#,
        )
        .expect("parses");
        assert_eq!(registry.agents.len(), 3);
        assert!(matches!(
            &registry.agents[1].distribution,
            Distribution::Npm { args, .. } if args == &["--acp"]
        ));
        assert!(!registry.agents[2].installable());
    }
}
