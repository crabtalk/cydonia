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
//! theme against a dark row. They are cached to disk and handed to
//! [`Icon::file`] as svg elements instead, which paint the shape's alpha
//! in whatever `text_color` the row already sets — the same path every other
//! icon in the app takes.

use crate::model::settings::{self, Agent};
use bezel::ui::icons::Icon;
use cacp_agents::{Distribution, Installed, registry};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub mod acp;
pub mod context;
pub mod mcp;
pub mod path;
pub mod serve;

/// Where the fetched catalog and the icons beside it are kept — a cache under
/// the config directory, not among the files a person edits.
pub fn cache_dir() -> Option<PathBuf> {
    settings::dir().ok().map(|dir| dir.join("cache"))
}

/// The registry's mark for each configured agent, by name.
///
/// Blocking: this reaches the network on a cold cache. Call it off the UI
/// thread. An empty map is the honest answer offline — every caller falls
/// back to what it drew before.
pub fn icons(configured: &[settings::Agent]) -> HashMap<String, Icon> {
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
            Some((entry.name.clone(), Icon::file(path)))
        })
        .collect()
}

/// The icon on disk, downloading it once.
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

/// One row of the agents section: what the registry publishes, and whether it
/// is on this machine.
pub struct Listing {
    pub agent: registry::Agent,
    /// The version on disk, when it is installed.
    pub installed: Option<String>,
    pub icon: Option<Icon>,
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
        .map(|agent| {
            let installed = Installed::find(&data, &agent.id).map(|found| found.version);
            let icon = cached(&dir, &agent.id).map(Icon::file);
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
        if let Some(url) = agent.icon.as_deref() {
            fetch(&dir, &agent.id, url);
        }
    }
}

/// How many of an installer's lines are kept. Enough that a failure's last
/// words are all there, and not so many that a chatty `npm` is held in full.
pub const OUTPUT_KEEP: usize = 40;

/// Add one line to what an installer has printed, keeping the tail.
///
/// The end is what anybody wants: the step it has reached while it runs, and
/// the words it failed with when it did. So a long install loses its opening
/// rather than its last say — see [`install`], which prints these.
pub fn record(held: &mut Vec<String>, line: String) {
    held.push(line);
    if held.len() > OUTPUT_KEEP {
        held.remove(0);
    }
}

/// Put the agent on disk and name it in `settings.toml`. Blocking: this runs
/// `npm`, or unpacks a release archive.
///
/// Both halves live here because only this function holds the registry entry,
/// and the package inside it is what tells `settings` which hand-written
/// `@latest` line this install supersedes.
///
/// `on_line` is handed each line the installer prints — `npm install <pkg>`,
/// `downloaded 24 MB`, `unpacking` — which is the only account of a step that
/// can run for a minute. It is called on whichever thread this is, so a caller
/// on the background executor sends rather than paints.
/// What an agent's install is agreed to, as a line that outlives a version.
///
/// A package by name and a download by the host that serves it: those are what
/// decide whose code ends up on the machine. The version is left out on
/// purpose — see [`settings::Settings::trusted_agents`].
///
/// An agent whose distribution cannot be installed still gets a mark, so that
/// a catalogue entry which becomes installable later is a fresh decision
/// rather than one already agreed to.
pub fn source_mark(agent: &registry::Agent) -> String {
    match &agent.distribution {
        Distribution::Npm { package, .. } => {
            format!("npm:{}", cacp_agents::package_name(package))
        }
        Distribution::Binary(binary) => format!("binary:{}", archive_host(&binary.archive)),
        Distribution::Unsupported { kind } => format!("unsupported:{kind}"),
    }
}

/// The host an archive is served from, or the whole URL where it has none to
/// read — a string that cannot be parsed is not one to quietly shorten.
fn archive_host(archive: &str) -> &str {
    let rest = archive
        .strip_prefix("https://")
        .or_else(|| archive.strip_prefix("http://"));
    match rest {
        Some(rest) => rest.split('/').next().unwrap_or(archive),
        None => archive,
    }
}

pub fn install(agent: &registry::Agent, on_line: impl FnMut(&str)) -> anyhow::Result<()> {
    let installed = agent.install(&settings::data_dir()?, on_line)?;
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

#[cfg(test)]
#[path = "../../tests/unit/agent_trust.rs"]
mod trust_tests;
