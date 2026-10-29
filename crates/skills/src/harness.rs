use crate::{Frontmatter, Scope, Skill, Switch, plugin::Plugin};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// The file a skill's directory holds.
const SKILL_FILE: &str = "SKILL.md";

/// An agent that reads skills from disk, and so has skills to list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Harness {
    Claude,
    Codex,
    Gemini,
    Cursor,
    OpenCode,
    Pi,
}

/// How far above a project an agent looks for one of its project skill
/// directories.
#[derive(Clone, Copy)]
enum Reach {
    /// The project's directory alone.
    Project,
    /// The project's directory and each above it up to its repository's root.
    Repository,
    /// The project's directory and every directory above it.
    Ancestors,
}

/// A directory of skills, one subdirectory each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub path: PathBuf,
    pub scope: Scope,
    /// The name of the plugin it is the `skills` of.
    pub plugin: Option<String>,
}

impl Harness {
    pub const ALL: [Self; 6] = [
        Self::Claude,
        Self::Codex,
        Self::Gemini,
        Self::Cursor,
        Self::OpenCode,
        Self::Pi,
    ];

    /// Its ids in the ACP registry.
    pub fn registry_ids(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["claude-acp"],
            Self::Codex => &["codex-acp"],
            Self::Gemini => &["gemini"],
            Self::Cursor => &["cursor"],
            Self::OpenCode => &["opencode", "opencode-v2"],
            Self::Pi => &["pi-acp"],
        }
    }

    /// The npm package its ACP adapter ships as, without a version. `None`
    /// for an agent the registry ships as a binary.
    pub fn package(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("@agentclientprotocol/claude-agent-acp"),
            Self::Codex => Some("@agentclientprotocol/codex-acp"),
            Self::Gemini => Some("@google/gemini-cli"),
            Self::Pi => Some("pi-acp"),
            Self::Cursor | Self::OpenCode => None,
        }
    }

    /// Where it keeps its own files: `$CLAUDE_CONFIG_DIR` or `~/.claude`,
    /// `$CODEX_HOME` or `~/.codex`, `$GEMINI_CLI_HOME/.gemini` or `~/.gemini`,
    /// `~/.cursor`, `~/.config/opencode`, `$PI_CODING_AGENT_DIR` or
    /// `~/.pi/agent`.
    pub fn home(self) -> Option<PathBuf> {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let under_home = |dir: &str| dirs::home_dir().map(|home| home.join(dir));
        match self {
            Self::Claude => var("CLAUDE_CONFIG_DIR").or_else(|| under_home(".claude")),
            Self::Codex => var("CODEX_HOME").or_else(|| under_home(".codex")),
            Self::Gemini => var("GEMINI_CLI_HOME")
                .map(|home| home.join(".gemini"))
                .or_else(|| under_home(".gemini")),
            Self::Cursor => under_home(".cursor"),
            Self::OpenCode => under_home(".config/opencode"),
            Self::Pi => var("PI_CODING_AGENT_DIR").or_else(|| under_home(".pi/agent")),
        }
    }

    /// Whether the library can switch its skills off.
    pub fn switches(self) -> bool {
        !matches!(self, Self::Cursor)
    }

    /// Its personal skill directories, by name under its home and under the
    /// user's home directory.
    fn personal(self) -> (&'static [&'static str], &'static [&'static str]) {
        match self {
            Self::Claude => (&["skills"], &[]),
            Self::Codex => (&[], &[".agents/skills"]),
            Self::Gemini | Self::Pi => (&["skills"], &[".agents/skills"]),
            Self::Cursor => (
                &["skills"],
                &[".agents/skills", ".claude/skills", ".codex/skills"],
            ),
            Self::OpenCode => (&["skills"], &[".claude/skills", ".agents/skills"]),
        }
    }

    /// Its project skill directories, relative to a project, and how far up
    /// from the project each is looked for.
    fn project(self) -> &'static [(&'static str, Reach)] {
        match self {
            Self::Claude => &[(".claude/skills", Reach::Repository)],
            Self::Codex => &[(".agents/skills", Reach::Repository)],
            Self::Gemini => &[
                (".gemini/skills", Reach::Project),
                (".agents/skills", Reach::Project),
            ],
            Self::Cursor => &[
                (".agents/skills", Reach::Project),
                (".cursor/skills", Reach::Project),
                (".claude/skills", Reach::Project),
                (".codex/skills", Reach::Project),
            ],
            Self::OpenCode => &[
                (".opencode/skills", Reach::Repository),
                (".claude/skills", Reach::Repository),
                (".agents/skills", Reach::Repository),
            ],
            Self::Pi => &[
                (".pi/skills", Reach::Project),
                (".agents/skills", Reach::Ancestors),
            ],
        }
    }

    /// The roots it reads skills from, personal first, then each project's. A
    /// root shared by two projects is listed once. Claude Code's also hold each
    /// account bucket under `skills/synced` and the `skills` of each plugin
    /// that loads.
    pub fn roots(self, projects: &[PathBuf]) -> Vec<Root> {
        let (in_home, in_user) = self.personal();
        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(home) = self.home() {
            roots.extend(in_home.iter().map(|dir| home.join(dir)));
        }
        if let Some(user) = dirs::home_dir() {
            roots.extend(in_user.iter().map(|dir| user.join(dir)));
        }
        if let (Self::Claude, Some(home)) = (self, self.home()) {
            roots.extend(subdirectories(&home.join("skills/synced")));
        }
        let mut roots: Vec<Root> = roots
            .into_iter()
            .map(|path| Root {
                path,
                scope: Scope::Personal,
                plugin: None,
            })
            .collect();
        for project in projects {
            for (dir, reach) in self.project() {
                for above in reach.dirs(project) {
                    roots.push(Root {
                        path: above.join(dir),
                        scope: Scope::Project(project.clone()),
                        plugin: None,
                    });
                }
            }
        }
        if let (Self::Claude, Some(home)) = (self, self.home()) {
            let scopes = std::iter::once(Scope::Personal)
                .chain(projects.iter().cloned().map(Scope::Project));
            for scope in scopes {
                for plugin in Plugin::loaded(&home, &scope) {
                    roots.push(Root {
                        path: plugin.root.join("skills"),
                        scope: scope.clone(),
                        plugin: Some(plugin.name),
                    });
                }
            }
        }
        let mut seen = HashSet::new();
        roots.retain(|root| seen.insert(root.path.clone()));
        roots
    }

    /// Blocking: every skill in [`Self::roots`], by name, and whether each is
    /// switched on.
    pub fn scan(self, projects: &[PathBuf]) -> Vec<Skill> {
        let mut skills = Vec::new();
        for root in self.roots(projects) {
            let dirs = skill_dirs(&root.path, self == Self::Pi);
            if dirs.is_empty() {
                continue;
            }
            let switch = Switch::load(self, &root.scope);
            for dir in dirs {
                let path = dir.join(SKILL_FILE);
                let Ok(source) = fs::read_to_string(&path) else {
                    continue;
                };
                let source = source.replace("\r\n", "\n");
                let meta = Frontmatter::parse(&source)
                    .and_then(Result::ok)
                    .map(|(meta, _)| meta)
                    .unwrap_or_default();
                let Some(name) = meta.name.clone().or_else(|| {
                    dir.file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned)
                }) else {
                    continue;
                };
                let name = match &root.plugin {
                    Some(plugin) => format!("{plugin}:{name}"),
                    None => name,
                };
                let enabled = root.plugin.is_some()
                    || switch
                        .as_ref()
                        .is_none_or(|switch| !switch.off(&name, &path));
                skills.push(Skill {
                    harness: self,
                    description: meta.summary(),
                    name,
                    path,
                    root: root.path.clone(),
                    scope: root.scope.clone(),
                    plugin: root.plugin.clone(),
                    enabled,
                });
            }
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }
}

/// The directories under `dir`, symlinks followed, hidden ones left out.
/// Empty where `dir` cannot be read.
pub(crate) fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.'))
                && path.is_dir()
        })
        .collect();
    dirs.sort();
    dirs
}

/// The skill directories under `root`: its subdirectories, or with `nested`
/// every directory below it holding a `SKILL.md` and not inside another.
fn skill_dirs(root: &Path, nested: bool) -> Vec<PathBuf> {
    if !nested {
        return subdirectories(root);
    }
    let mut found = Vec::new();
    for dir in subdirectories(root) {
        match dir.join(SKILL_FILE).is_file() {
            true => found.push(dir),
            false => found.extend(skill_dirs(&dir, true)),
        }
    }
    found
}

impl Reach {
    /// The directories it covers for `project`, nearest first.
    fn dirs(self, project: &Path) -> Vec<&Path> {
        match self {
            Self::Project => vec![project],
            Self::Ancestors => project.ancestors().collect(),
            Self::Repository => {
                let mut dirs = Vec::new();
                for dir in project.ancestors() {
                    dirs.push(dir);
                    if dir.join(".git").exists() {
                        return dirs;
                    }
                }
                vec![project]
            }
        }
    }
}
