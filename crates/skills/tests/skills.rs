use cydonia_skills::{Frontmatter, Harness, Scope};
use std::{fs, path::PathBuf};

/// A scratch directory standing in for the home directory and a project.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-skills-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("home")).unwrap();
        fs::create_dir_all(path.join("project/.git")).unwrap();
        // Safety: nextest runs each test in a process of its own.
        unsafe {
            std::env::set_var("HOME", path.join("home"));
            std::env::set_var("CLAUDE_CONFIG_DIR", path.join("home/.claude"));
            std::env::set_var("CODEX_HOME", path.join("home/.codex"));
        }
        Self(path)
    }

    fn skill(&self, root: &str, dir: &str, source: &str) {
        let dir = self.0.join(root).join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), source).unwrap();
    }

    fn project(&self) -> PathBuf {
        self.0.join("project")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn frontmatter_is_read_leniently() {
    let (meta, body) =
        Frontmatter::parse("---\nname: a-b\ndescription: |\n  one\n  two\n---\nbody\n")
            .unwrap()
            .unwrap();
    assert_eq!(meta.name.as_deref(), Some("a-b"));
    assert_eq!(meta.summary().as_deref(), Some("one two"));
    assert_eq!(body, "body\n");
    assert!(Frontmatter::parse("no block").is_none());
    assert!(Frontmatter::valid_name("a-b"));
    assert!(!Frontmatter::valid_name("a--b"));
    assert!(!Frontmatter::valid_name("A"));
}

#[test]
fn claude_scans_personal_synced_and_project_skills() {
    let scratch = Scratch::new("claude-scan");
    scratch.skill(
        "home/.claude/skills",
        "mine",
        "---\nname: mine\ndescription: Mine.\n---\n",
    );
    scratch.skill(
        "home/.claude/skills/synced/bucket",
        "account",
        "no frontmatter\n",
    );
    scratch.skill(
        "project/.claude/skills",
        "local",
        "---\nname: renamed\n---\n",
    );
    scratch.skill(
        "project/.agents/skills",
        "codex-only",
        "---\nname: codex-only\n---\n",
    );
    let skills = Harness::Claude.scan(&[scratch.project()]);
    let names: Vec<_> = skills.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["account", "mine", "renamed"]);
    let renamed = skills.iter().find(|skill| skill.name == "renamed").unwrap();
    assert_eq!(renamed.scope, Scope::Project(scratch.project()));
    let mine = skills.iter().find(|skill| skill.name == "mine").unwrap();
    assert_eq!(mine.description.as_deref(), Some("Mine."));
    assert_eq!(mine.scope, Scope::Personal);
}
