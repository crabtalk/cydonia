//! The MCP servers offered to every agent, and the registry they come from.
//!
//! `mcp.toml` is machine-written, like [`crate::model::state`]: the settings
//! window owns it and rewrites it whole. Everything the file can hold is a
//! field on [`McpServer`], so a hand-added `env` survives that round trip —
//! comments do not.
//!
//! Searching is not cached because [`registry::search`] is not cacheable: the
//! catalog runs to thousands of entries and the query is answered server-side.

use crate::model::settings;
use anyhow::{Context, Result};
use cacp_agents::mcp::{self as registry, Distribution};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// One MCP server offered to agents: either a local `command args...` over
/// stdio, or a remote `url`, which needs the agent to advertise HTTP MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The registry id this came from, when it came from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

const fn enabled_by_default() -> bool {
    true
}

impl McpServer {
    /// What the row shows under the name: the address, whichever kind it is.
    pub fn address(&self) -> String {
        match (&self.command, &self.url) {
            (Some(command), _) => match self.args.is_empty() {
                true => command.clone(),
                false => format!("{command} {}", self.args.join(" ")),
            },
            (None, Some(url)) => url.clone(),
            (None, None) => String::new(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    servers: Vec<McpServer>,
}

fn path() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("mcp.toml"))
}

/// Every MCP server the user has added, enabled or not.
pub fn servers() -> Vec<McpServer> {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|body| toml::from_str::<Store>(&body).ok())
        .map(|store| store.servers)
        .unwrap_or_default()
}

fn save(servers: Vec<McpServer>) -> Result<()> {
    let path = path().context("no config directory on this system")?;
    let body = toml::to_string_pretty(&Store { servers })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))
}

/// Add `server`, or replace the one already under its name — the name is what
/// the agent sees, so two of them would be one server as far as it can tell.
pub fn put(server: McpServer) -> Result<()> {
    let mut servers = servers();
    match servers.iter_mut().find(|held| held.name == server.name) {
        Some(held) => *held = server,
        None => servers.push(server),
    }
    save(servers)
}

pub fn remove(name: &str) -> Result<()> {
    let mut servers = servers();
    servers.retain(|server| server.name != name);
    save(servers)
}

pub fn set_enabled(name: &str, enabled: bool) -> Result<()> {
    let mut servers = servers();
    if let Some(server) = servers.iter_mut().find(|server| server.name == name) {
        server.enabled = enabled;
    }
    save(servers)
}

/// Put the server where an agent can reach it and name it in `mcp.toml`.
/// Blocking: an npm-distributed server is installed here.
pub fn install(server: &registry::Server) -> Result<()> {
    let command = server.install(&settings::data_dir()?, |_| {})?;
    let url = match &server.distribution {
        Distribution::Remote { url } => Some(url.clone()),
        _ => None,
    };
    put(McpServer {
        name: server.name.clone(),
        enabled: true,
        command,
        args: Vec::new(),
        env: BTreeMap::new(),
        url,
        id: Some(server.id.clone()),
    })
}

/// A server typed in by hand. The two shapes the wire has are stdio and HTTP,
/// so the address decides which this is rather than a control asking.
pub fn from_address(name: String, address: &str) -> McpServer {
    let address = address.trim();
    let remote = address.starts_with("http://") || address.starts_with("https://");
    let mut words = address.split_whitespace().map(str::to_owned);
    McpServer {
        name,
        enabled: true,
        command: (!remote).then(|| words.next().unwrap_or_default()),
        args: match remote {
            true => Vec::new(),
            false => words.collect(),
        },
        env: BTreeMap::new(),
        url: remote.then(|| address.to_owned()),
        id: None,
    }
}
