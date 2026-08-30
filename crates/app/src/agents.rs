//! The registry's icon for each agent you have configured.
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

use crate::model::settings;
use bezel::gpui::SharedString;
use cacp_agents::{Distribution, registry};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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
    let published: HashMap<&str, (&str, &str)> = catalog
        .agents
        .iter()
        .filter_map(|agent| match (&agent.distribution, &agent.icon) {
            (Distribution::Npm { package, .. }, Some(icon)) => Some((
                cacp_agents::package_name(package),
                (agent.id.as_str(), icon.as_str()),
            )),
            _ => None,
        })
        .collect();

    let dir = cache.join("icons");
    configured
        .iter()
        .filter_map(|entry| {
            let (id, url) = entry
                .args
                .iter()
                .find_map(|arg| published.get(cacp_agents::package_name(arg)))?;
            let path = fetch(&dir, id, url)?;
            Some((entry.name.clone(), SharedString::from(path)))
        })
        .collect()
}

/// The icon on disk, downloading it once. The asset path is the file's own
/// path — [`crate::assets`] reads it back by that name, so nothing has to keep
/// a second table mapping one to the other.
fn fetch(dir: &Path, id: &str, url: &str) -> Option<String> {
    let path = dir.join(format!("{id}.svg"));
    if !path.exists() {
        let body = ureq::get(url).call().ok()?.body_mut().read_to_vec().ok()?;
        std::fs::create_dir_all(dir).ok()?;
        std::fs::write(&path, body).ok()?;
    }
    Some(path.to_str()?.to_owned())
}
