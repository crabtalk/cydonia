//! The agents this machine can run: the catalog, what is installed of it,
//! and each one's icon. [`acp`] speaks to a running agent; [`mcp`] is the
//! tool servers they are all offered.
//!
//! Settings name a command, the catalog names a package, and the two meet at
//! the npm package: the registry pins `…/claude-agent-acp@0.70.0` where a
//! settings entry says `@latest`, so both sides go through
//! [`cacp_agents::package_name`] before they are compared. Matching on the
//! package rather than on `command` covers `npx`, `bunx` and anything else
//! that takes the package as an argument — and an agent that is a binary on
//! this machine matches nothing, which is the right answer.
//!
//! Every icon the registry publishes is a `currentColor` glyph, so it carries
//! no colour of its own: rasterised as an image it comes out black, on a dark
//! theme against a dark row. They are cached to disk and drawn through
//! [`crate::assets`] as svg elements instead, which paint the shape's alpha in
//! whatever `text_color` the row already sets — the same path every other icon
//! in the app takes.

use crate::model::settings::{self, Agent};
use bezel::gpui::SharedString;
use cacp_agents::{Distribution, Installed, registry};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub mod acp;
pub mod mcp;

/// Where the fetched catalog and the icons beside it are kept — a cache under
/// the config directory, not among the files a person edits.
pub fn cache_dir() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("cache"))
}

/// Icon asset paths by configured agent name.
///
/// Blocking: this reaches the network on a cold cache. Call it off the UI
/// thread. An empty map is the honest answer offline — every caller falls
/// back to what it drew before.
pub fn icons(configured: &[settings::Agent]) -> HashMap<String, SharedString> {
    let Some(cache) = cache_dir() else {
        return HashMap::new();
    };
    let Some(catalog) = registry::catalog(&cache) else {
        return HashMap::new();
    };
    // Two ways in, because a settings entry arrives two ways. An installed
    // one carries the registry id outright. A hand-written one names the npm
    // package and nothing else, so it is matched on that — which is also all
    // an installed entry used to have, until installing moved the package
    // into the command path and left `args` empty.
    let icons: HashMap<&str, &str> = catalog
        .agents
        .iter()
        .filter_map(|agent| Some((agent.id.as_str(), agent.icon.as_deref()?)))
        .collect();
    let by_package: HashMap<&str, &str> = catalog
        .agents
        .iter()
        .filter_map(|agent| match &agent.distribution {
            Distribution::Npm { package, .. } => {
                Some((cacp_agents::package_name(package), agent.id.as_str()))
            }
            _ => None,
        })
        .collect();

    let dir = cache.join("icons");
    configured
        .iter()
        .filter_map(|entry| {
            let id = match entry.id.as_deref() {
                Some(id) => id,
                None => entry
                    .args
                    .iter()
                    .find_map(|arg| by_package.get(cacp_agents::package_name(arg)).copied())?,
            };
            let path = fetch(&dir, id, icons.get(id)?)?;
            Some((entry.name.clone(), SharedString::from(path)))
        })
        .collect()
}

/// The icon on disk, downloading it once. The asset path is the file's own
/// path — [`crate::assets`] reads it back by that name, so nothing has to keep
/// a second table mapping one to the other.
fn fetch(dir: &Path, id: &str, url: &str) -> Option<String> {
    if let Some(path) = cached(dir, id) {
        return Some(path);
    }
    let path = cacp_agents::contained(dir, &format!("{id}.svg")).ok()?;
    let body = ureq::get(url).call().ok()?.body_mut().read_to_vec().ok()?;
    std::fs::create_dir_all(dir).ok()?;
    std::fs::write(&path, body).ok()?;
    Some(path.to_str()?.to_owned())
}

/// The icon already on disk, if it is.
fn cached(dir: &Path, id: &str) -> Option<String> {
    let path = cacp_agents::contained(dir, &format!("{id}.svg")).ok()?;
    path.exists().then(|| path.to_str())?.map(str::to_owned)
}

// ── the catalog, for the settings window ─────────────────────────

/// The adapters whose authors are the labs that build the models. Named one by
/// one because each pulls a node runtime down with it — the catalog takes any
/// publisher who submits one, and running their npm package is running their
/// code.
const ALLOWED: [&str; 4] = ["claude-acp", "codex-acp", "gemini", "antigravity-acp"];

/// Whether the catalog entry is one the agents section offers: those four, and
/// every agent that ships a native binary — a release archive needs nothing on
/// this machine but the download, and `Distribution::Binary` is already
/// narrowed to a build this machine can run.
///
/// One predicate, because both [`listings`] and [`prefetch_icons`] have to
/// answer it the same way: a row this admits and the prefetch skips is a row
/// that never finds its mark.
fn listed(agent: &registry::Agent) -> bool {
    ALLOWED.contains(&agent.id.as_str()) || matches!(agent.distribution, Distribution::Binary(_))
}

/// One row of the agents section: what the registry publishes, and whether it
/// is on this machine.
pub struct Listing {
    pub agent: registry::Agent,
    /// The version on disk, when it is installed.
    pub installed: Option<String>,
    pub icon: Option<SharedString>,
}

/// The whole catalog with each entry's local state. Blocking on the registry,
/// but never on an icon: a mark is used only if it is already on disk, so the
/// list arrives in one round trip rather than forty. [`prefetch_icons`] is
/// what fills the gaps in.
pub fn listings() -> Vec<Listing> {
    let (Some(cache), Ok(data)) = (cache_dir(), settings::data_dir()) else {
        return Vec::new();
    };
    let Some(catalog) = registry::catalog(&cache) else {
        return Vec::new();
    };
    let dir = cache.join("icons");
    catalog
        .agents
        .into_iter()
        .filter(listed)
        .map(|agent| {
            let installed = Installed::find(&data, &agent.id).map(|found| found.version);
            let icon = cached(&dir, &agent.id).map(SharedString::from);
            Listing {
                agent,
                installed,
                icon,
            }
        })
        .collect()
}

/// Download every catalog icon that is not already on disk. Blocking, and
/// slow on a cold cache — one request per agent — so it belongs behind a list
/// that is already on screen.
pub fn prefetch_icons() {
    let Some(cache) = cache_dir() else {
        return;
    };
    let Some(catalog) = registry::catalog(&cache) else {
        return;
    };
    let dir = cache.join("icons");
    for agent in &catalog.agents {
        if !listed(agent) {
            continue;
        }
        if let Some(url) = agent.icon.as_deref() {
            fetch(&dir, &agent.id, url);
        }
    }
}

/// Put the agent on disk and name it in `settings.toml`. Blocking: this runs
/// `npm`, or unpacks a release archive.
///
/// Both halves live here because only this function holds the registry entry,
/// and the package inside it is what tells `settings` which hand-written
/// `@latest` line this install supersedes.
pub fn install(agent: &registry::Agent) -> anyhow::Result<()> {
    let installed = agent.install(&settings::data_dir()?, |_| {})?;
    let entry = Agent {
        name: agent.name.clone(),
        id: Some(agent.id.clone()),
        command: installed.command,
        args: installed.args,
        env: installed.env,
    };
    let package = match &agent.distribution {
        Distribution::Npm { package, .. } => Some(cacp_agents::package_name(package)),
        _ => None,
    };
    settings::put_agent(&entry, package)
}

/// Take it off disk and out of `settings.toml`.
pub fn remove(id: &str) -> anyhow::Result<()> {
    Installed::remove(&settings::data_dir()?, id)?;
    settings::remove_agent(id)
}
