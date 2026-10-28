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

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap_or_default()
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
    assert!(skills.iter().all(|skill| skill.enabled));
    let renamed = skills.iter().find(|skill| skill.name == "renamed").unwrap();
    assert_eq!(renamed.scope, Scope::Project(scratch.project()));
    let mine = skills.iter().find(|skill| skill.name == "mine").unwrap();
    assert_eq!(mine.description.as_deref(), Some("Mine."));
    assert_eq!(mine.scope, Scope::Personal);
}

#[test]
fn claude_switches_through_skill_overrides() {
    let scratch = Scratch::new("claude-switch");
    scratch.skill(
        "project/.claude/skills",
        "deploy",
        "---\nname: deploy\n---\n",
    );
    fs::write(
        scratch.0.join("project/.claude/settings.json"),
        "{\"skillOverrides\": {\"deploy\": \"off\"}}",
    )
    .unwrap();
    let project = [scratch.project()];
    let skill = Harness::Claude.scan(&project).remove(0);
    assert!(!skill.enabled);

    // On over a shared `off`: the local file says `on`.
    skill.set_enabled(true).unwrap();
    assert!(Harness::Claude.scan(&project)[0].enabled);
    assert!(
        scratch
            .read("project/.claude/settings.local.json")
            .contains("\"deploy\": \"on\"")
    );

    skill.set_enabled(false).unwrap();
    assert!(!Harness::Claude.scan(&project)[0].enabled);

    // With nothing below saying `off`, on drops the entry.
    fs::write(scratch.0.join("project/.claude/settings.json"), "{}").unwrap();
    skill.set_enabled(true).unwrap();
    assert!(Harness::Claude.scan(&project)[0].enabled);
    assert!(
        !scratch
            .read("project/.claude/settings.local.json")
            .contains("skillOverrides")
    );
}

#[test]
fn codex_switches_through_skills_config() {
    let scratch = Scratch::new("codex-switch");
    scratch.skill("home/.agents/skills", "lint", "---\nname: lint\n---\n");
    fs::create_dir_all(scratch.0.join("home/.codex")).unwrap();
    fs::write(
        scratch.0.join("home/.codex/config.toml"),
        "# mine\nmodel = \"o3\"\n",
    )
    .unwrap();
    let skill = Harness::Codex.scan(&[]).remove(0);
    assert!(skill.enabled);

    skill.set_enabled(false).unwrap();
    assert!(!Harness::Codex.scan(&[])[0].enabled);
    let config = scratch.read("home/.codex/config.toml");
    assert!(config.starts_with("# mine\nmodel = \"o3\"\n"));
    assert!(config.contains("[[skills.config]]"));
    assert!(config.contains("enabled = false"));

    skill.set_enabled(true).unwrap();
    assert!(Harness::Codex.scan(&[])[0].enabled);
    assert_eq!(
        scratch.read("home/.codex/config.toml"),
        "# mine\nmodel = \"o3\"\n"
    );
}
