use crate::{Frontmatter, Scope, Skill, Switch};
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
}

/// A directory of skills, one subdirectory each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub path: PathBuf,
    pub scope: Scope,
}

impl Harness {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    /// Its id in the ACP registry.
    pub fn registry_id(self) -> &'static str {
        match self {
            Self::Claude => "claude-acp",
            Self::Codex => "codex-acp",
        }
    }

    /// The npm package its ACP adapter ships as, without a version.
    pub fn package(self) -> &'static str {
        match self {
            Self::Claude => "@agentclientprotocol/claude-agent-acp",
            Self::Codex => "@agentclientprotocol/codex-acp",
        }
    }

    /// Where it keeps its own files: `$CLAUDE_CONFIG_DIR` or `~/.claude`,
    /// `$CODEX_HOME` or `~/.codex`.
    pub fn home(self) -> Option<PathBuf> {
        let (var, default) = match self {
            Self::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
            Self::Codex => ("CODEX_HOME", ".codex"),
        };
        std::env::var_os(var)
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(default)))
    }

    /// The directory a project holds its skills in, relative to the project.
    fn project_dir(self) -> &'static str {
        match self {
            Self::Claude => ".claude/skills",
            Self::Codex => ".agents/skills",
        }
    }

    /// The roots it reads skills from, personal first, then each project's
    /// from the project's directory up to its repository's root. A root shared
    /// by two projects is listed once.
    ///
    /// Claude Code: `~/.claude/skills`, each account bucket under
    /// `~/.claude/skills/synced`, and `.claude/skills`. Codex: `~/.agents/skills`
    /// and `.agents/skills`. Plugin skills are not among them.
    pub fn roots(self, projects: &[PathBuf]) -> Vec<Root> {
        let mut roots = Vec::new();
        match self {
            Self::Claude => {
                if let Some(skills) = self.home().map(|home| home.join("skills")) {
                    let synced = subdirectories(&skills.join("synced"));
                    roots.push(skills);
                    roots.extend(synced);
                }
            }
            Self::Codex => roots.extend(dirs::home_dir().map(|home| home.join(".agents/skills"))),
        }
        let mut roots: Vec<Root> = roots
            .into_iter()
            .map(|path| Root {
                path,
                scope: Scope::Personal,
            })
            .collect();
        for project in projects {
            for dir in upward(project) {
                roots.push(Root {
                    path: dir.join(self.project_dir()),
                    scope: Scope::Project(project.clone()),
                });
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
            let dirs = subdirectories(&root.path);
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
                let enabled = switch
                    .as_ref()
                    .is_none_or(|switch| !switch.off(&name, &path));
                skills.push(Skill {
                    harness: self,
                    description: meta.summary(),
                    name,
                    path,
                    root: root.path.clone(),
                    scope: root.scope.clone(),
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
fn subdirectories(dir: &Path) -> Vec<PathBuf> {
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

/// `project` and each directory above it up to the repository it is in, the
/// one holding `.git`. Only `project` outside a repository.
fn upward(project: &Path) -> Vec<&Path> {
    let mut dirs = Vec::new();
    for dir in project.ancestors() {
        dirs.push(dir);
        if dir.join(".git").exists() {
            return dirs;
        }
    }
    vec![project]
}
