//! The official MCP registry: search it, and install what it lists.
//!
//! Searching is server-side and live — the catalog runs to thousands of
//! entries, so there is nothing to cache locally.

use crate::install;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub const SEARCH_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";

/// How many results one search returns.
const SEARCH_LIMIT: usize = 30;

/// One MCP server from the registry.
#[derive(Debug, Clone)]
pub struct Server {
    /// Reverse-DNS registry name, e.g. `io.github.owner/thing`.
    pub id: String,
    /// Short display name (the last path segment of `id`).
    pub name: String,
    pub description: String,
    pub distribution: Distribution,
}

/// How to reach an MCP server.
#[derive(Debug, Clone)]
pub enum Distribution {
    /// An npm package the agent runs over stdio.
    Npm { package: String },
    /// A remote server the agent connects to over HTTP.
    Remote { url: String },
    /// Python, containers, and other kinds that cannot be launched yet.
    Unsupported { kind: String },
}

impl Server {
    pub fn installable(&self) -> bool {
        !matches!(self.distribution, Distribution::Unsupported { .. })
    }

    /// Remote servers need the agent to advertise `mcp_capabilities.http`.
    pub fn is_remote(&self) -> bool {
        matches!(self.distribution, Distribution::Remote { .. })
    }

    /// Make this server launchable: npm packages are installed and their
    /// executable returned; remote servers need nothing, and give `None`.
    pub fn install(&self, data_dir: &Path, on_line: impl FnMut(&str)) -> Result<Option<String>> {
        match &self.distribution {
            Distribution::Npm { package } => {
                let dir = server_dir(data_dir, &self.id);
                Ok(Some(install::install_npm(&dir, package, on_line)?))
            }
            Distribution::Remote { .. } => Ok(None),
            Distribution::Unsupported { kind } => {
                bail!(
                    "{} is distributed as {kind}, which cannot be launched yet",
                    self.name
                )
            }
        }
    }
}

#[derive(Deserialize)]
struct WireResponse {
    #[serde(default)]
    servers: Vec<WireEntry>,
}

#[derive(Deserialize)]
struct WireEntry {
    server: WireServer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireServer {
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    packages: Vec<WirePackage>,
    #[serde(default)]
    remotes: Vec<WireRemote>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePackage {
    #[serde(default)]
    registry_type: String,
    identifier: String,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRemote {
    #[serde(rename = "type", default)]
    kind: String,
    url: String,
}

fn convert(server: WireServer) -> Server {
    // Prefer a package we can launch over stdio: every agent supports stdio,
    // while HTTP depends on the agent's capabilities.
    let npm = server
        .packages
        .iter()
        .find(|p| p.registry_type == "npm")
        .map(|p| Distribution::Npm {
            package: match &p.version {
                Some(version) if !version.is_empty() => {
                    format!("{}@{}", p.identifier, version)
                }
                _ => p.identifier.clone(),
            },
        });
    let remote = server
        .remotes
        .iter()
        .find(|r| r.kind == "streamable-http")
        .map(|r| Distribution::Remote { url: r.url.clone() });
    let distribution = npm.or(remote).unwrap_or_else(|| Distribution::Unsupported {
        kind: server
            .packages
            .first()
            .map(|p| p.registry_type.clone())
            .or_else(|| server.remotes.first().map(|r| r.kind.clone()))
            .unwrap_or_else(|| "unknown".to_owned()),
    });

    let name = server.title.clone().unwrap_or_else(|| {
        server
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&server.name)
            .to_owned()
    });
    Server {
        id: server.name,
        name,
        description: server.description,
        distribution,
    }
}

/// Read a search response, for a caller that fetched it itself.
pub fn parse(body: &str) -> Result<Vec<Server>> {
    let response: WireResponse =
        serde_json::from_str(body).context("malformed MCP registry response")?;
    Ok(dedupe(
        response.servers.into_iter().map(|e| convert(e.server)),
    ))
}

/// Search the registry. An empty query returns the newest entries.
///
/// `version=latest` is essential: without it the registry returns every
/// published version of every server, so one name appears many times.
pub fn search(query: &str) -> Result<Vec<Server>> {
    let mut request = ureq::get(SEARCH_URL)
        .query("limit", SEARCH_LIMIT.to_string())
        .query("version", "latest");
    if !query.trim().is_empty() {
        request = request.query("search", query.trim());
    }
    let body = request
        .call()
        .context("searching the MCP registry")?
        .body_mut()
        .read_to_string()
        .context("reading MCP registry results")?;
    parse(&body)
}

/// Keep the first entry per id — belt and braces behind `version=latest`.
fn dedupe(servers: impl Iterator<Item = Server>) -> Vec<Server> {
    let mut seen = HashSet::new();
    servers.filter(|s| seen.insert(s.id.clone())).collect()
}

/// Where an installed MCP server lives.
fn server_dir(data_dir: &Path, id: &str) -> PathBuf {
    // Registry ids contain `/`, which would nest directories.
    data_dir.join("mcp").join(id.replace('/', "_"))
}
