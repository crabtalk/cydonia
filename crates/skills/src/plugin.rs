use crate::{Scope, claude::Settings, harness};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The file under a plugin's root that marks it one, and an installed version
/// Claude Code has moved past.
const MANIFEST: &str = ".claude-plugin/plugin.json";
const ORPHANED: &str = ".orphaned_at";

/// The fields of a plugin's manifest the skills listing reads.
#[derive(Default, Deserialize)]
struct Manifest {
    name: Option<String>,
    #[serde(rename = "defaultEnabled")]
    default_enabled: Option<bool>,
}

impl Manifest {
    fn read(root: &Path) -> Option<Self> {
        let text = fs::read_to_string(root.join(MANIFEST)).ok()?;
        Some(serde_json::from_str(&text).unwrap_or_default())
    }
}

/// A Claude Code plugin on disk. Its skills are under `skills/` in its root,
/// and listed as `name:skill`.
pub(crate) struct Plugin {
    /// `<entry>@<marketplace>`, `<name>@synced` or `<name>@skills-dir`: what
    /// `enabledPlugins` keys it by.
    id: String,
    /// The manifest's name, which its skills are namespaced under.
    pub(crate) name: String,
    pub(crate) root: PathBuf,
    /// Whether it loads where no `enabledPlugins` names it.
    default_enabled: bool,
}

impl Plugin {
    /// Claude Code's plugins root: `$CLAUDE_CODE_PLUGIN_CACHE_DIR`, else
    /// `plugins` under its home.
    fn plugins_root(home: &Path) -> PathBuf {
        std::env::var_os("CLAUDE_CODE_PLUGIN_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("plugins"))
    }

    /// The plugins one scope sees: for the personal scope, those installed
    /// from marketplaces, those synced from claude.ai and those under the
    /// user's `skills`; for a project, those under its `.claude/skills`.
    fn on_disk(home: &Path, scope: &Scope) -> Vec<Self> {
        let mut plugins = Vec::new();
        let in_place = |dir: &Path, origin: &str| -> Option<Self> {
            let manifest = Manifest::read(dir)?;
            let name = manifest.name.or_else(|| dir_name(dir).map(str::to_owned))?;
            Some(Self {
                id: format!("{name}@{origin}"),
                name,
                root: dir.to_owned(),
                default_enabled: manifest.default_enabled.unwrap_or(true),
            })
        };
        match scope {
            Scope::Personal => {
                let root = Self::plugins_root(home);
                // `cache/<marketplace>/<entry>/<version>`: the version in use
                // is the one not marked orphaned.
                for marketplace in harness::subdirectories(&root.join("cache")) {
                    for entry in harness::subdirectories(&marketplace) {
                        let Some(version) = harness::subdirectories(&entry)
                            .into_iter()
                            .filter(|version| !version.join(ORPHANED).exists())
                            .max_by_key(|version| {
                                fs::metadata(version).and_then(|meta| meta.modified()).ok()
                            })
                        else {
                            continue;
                        };
                        let (Some(market), Some(entry_name)) =
                            (dir_name(&marketplace), dir_name(&entry))
                        else {
                            continue;
                        };
                        let manifest = Manifest::read(&version).unwrap_or_default();
                        plugins.push(Self {
                            id: format!("{entry_name}@{market}"),
                            name: manifest.name.unwrap_or_else(|| entry_name.to_owned()),
                            root: version,
                            default_enabled: false,
                        });
                    }
                }
                for bucket in harness::subdirectories(&root.join("synced")) {
                    plugins.extend(
                        harness::subdirectories(&bucket)
                            .iter()
                            .filter_map(|dir| in_place(dir, "synced")),
                    );
                }
                plugins.extend(
                    harness::subdirectories(&home.join("skills"))
                        .iter()
                        .filter_map(|dir| in_place(dir, "skills-dir")),
                );
            }
            Scope::Project(project) => plugins.extend(
                harness::subdirectories(&project.join(".claude/skills"))
                    .iter()
                    .filter_map(|dir| in_place(dir, "skills-dir")),
            ),
        }
        plugins
    }

    /// The plugins whose skills load in `scope`: for a project, the ones its
    /// settings turn on that the personal scope does not already load.
    pub(crate) fn loaded(home: &Path, scope: &Scope) -> Vec<Self> {
        let settings = Settings::load(home, scope);
        let on = |plugin: &Self, settings: &Settings| {
            settings
                .plugin(&plugin.id)
                .unwrap_or(plugin.default_enabled)
        };
        match scope {
            Scope::Personal => Self::on_disk(home, scope)
                .into_iter()
                .filter(|plugin| on(plugin, &settings))
                .collect(),
            Scope::Project(_) => {
                let personal = Settings::load(home, &Scope::Personal);
                Self::on_disk(home, &Scope::Personal)
                    .into_iter()
                    .filter(|plugin| !on(plugin, &personal))
                    .chain(Self::on_disk(home, scope))
                    .filter(|plugin| on(plugin, &settings))
                    .collect()
            }
        }
    }
}

fn dir_name(dir: &Path) -> Option<&str> {
    dir.file_name().and_then(|name| name.to_str())
}
