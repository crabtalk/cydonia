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
            std::env::set_var("GEMINI_CLI_HOME", path.join("home"));
            std::env::set_var("PI_CODING_AGENT_DIR", path.join("home/.pi/agent"));
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

#[test]
fn claude_lists_the_skills_of_plugins_that_load() {
    let scratch = Scratch::new("claude-plugins");
    let plugin = |root: &str, name: &str, manifest: &str| {
        let dir = scratch.0.join(root).join(name);
        fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        fs::write(dir.join(".claude-plugin/plugin.json"), manifest).unwrap();
    };
    // Installed from a marketplace: on only where enabledPlugins says so, and
    // read from the version not marked orphaned.
    plugin(
        "home/.claude/plugins/cache/market/tools",
        "1.0.0",
        "{\"name\": \"tools\"}",
    );
    fs::write(
        scratch
            .0
            .join("home/.claude/plugins/cache/market/tools/1.0.0/.orphaned_at"),
        "",
    )
    .unwrap();
    plugin(
        "home/.claude/plugins/cache/market/tools",
        "2.0.0",
        "{\"name\": \"tools\"}",
    );
    scratch.skill(
        "home/.claude/plugins/cache/market/tools/1.0.0/skills",
        "old",
        "---\nname: old\n---\n",
    );
    scratch.skill(
        "home/.claude/plugins/cache/market/tools/2.0.0/skills",
        "lint",
        "---\nname: lint\n---\n",
    );
    // Synced from claude.ai: on unless turned off.
    plugin(
        "home/.claude/plugins/synced/bucket",
        "support",
        "{\"name\": \"support\"}",
    );
    scratch.skill(
        "home/.claude/plugins/synced/bucket/support/skills",
        "triage",
        "---\nname: triage\n---\n",
    );
    // In the project's skills: on unless its manifest or settings say otherwise.
    plugin(
        "project/.claude/skills",
        "kit",
        "{\"name\": \"kit\", \"defaultEnabled\": false}",
    );
    scratch.skill(
        "project/.claude/skills/kit/skills",
        "ship",
        "---\nname: ship\n---\n",
    );
    fs::write(
        scratch.0.join("project/.claude/settings.json"),
        "{\"enabledPlugins\": {\"tools@market\": true, \"kit@skills-dir\": true}}",
    )
    .unwrap();

    let project = [scratch.project()];
    let skills = Harness::Claude.scan(&project);
    let names: Vec<_> = skills.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["kit:ship", "support:triage", "tools:lint"]);
    assert!(
        skills
            .iter()
            .all(|skill| skill.plugin.is_some() && skill.enabled)
    );
    let lint = skills
        .iter()
        .find(|skill| skill.name == "tools:lint")
        .unwrap();
    assert_eq!(lint.scope, Scope::Project(scratch.project()));
    let triage = skills
        .iter()
        .find(|skill| skill.name == "support:triage")
        .unwrap();
    assert_eq!(triage.scope, Scope::Personal);
    assert!(lint.set_enabled(false).is_err());
}

#[test]
fn an_unreadable_settings_file_is_never_written() {
    let scratch = Scratch::new("unreadable");
    scratch.skill("home/.claude/skills", "mine", "---\nname: mine\n---\n");
    fs::write(scratch.0.join("home/.claude/settings.json"), "{ // mine\n}").unwrap();
    let skill = Harness::Claude.scan(&[]).remove(0);
    assert!(skill.set_enabled(false).is_err());
    assert_eq!(scratch.read("home/.claude/settings.json"), "{ // mine\n}");
}

#[test]
fn gemini_switches_through_skills_disabled() {
    let scratch = Scratch::new("gemini-switch");
    scratch.skill("home/.gemini/skills", "mine", "---\nname: mine\n---\n");
    scratch.skill(
        "project/.agents/skills",
        "shared",
        "---\nname: shared\n---\n",
    );
    fs::create_dir_all(scratch.0.join("project/.gemini")).unwrap();
    fs::write(
        scratch.0.join("home/.gemini/settings.json"),
        "{\"skills\": {\"disabled\": [\"shared\"]}}",
    )
    .unwrap();
    let project = [scratch.project()];
    let skills = Harness::Gemini.scan(&project);
    let names: Vec<_> = skills.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["mine", "shared"]);
    let shared = skills[1].clone();
    assert!(!shared.enabled && shared.switchable());

    // On takes it out of every file listing it.
    shared.set_enabled(true).unwrap();
    assert!(Harness::Gemini.scan(&project)[1].enabled);
    assert!(
        !scratch
            .read("home/.gemini/settings.json")
            .contains("\"shared\"")
    );

    // Off lists it in the project's own file.
    shared.set_enabled(false).unwrap();
    assert!(!Harness::Gemini.scan(&project)[1].enabled);
    assert!(
        scratch
            .read("project/.gemini/settings.json")
            .contains("\"shared\"")
    );
}

#[test]
fn pi_finds_nested_skills_and_switches_through_exclusions() {
    let scratch = Scratch::new("pi");
    scratch.skill(
        "home/.pi/agent/skills/group",
        "deep",
        "---\nname: deep\n---\n",
    );
    scratch.skill("project/.pi/skills", "local", "---\nname: local\n---\n");
    let project = [scratch.project()];
    let skills = Harness::Pi.scan(&project);
    let names: Vec<_> = skills.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["deep", "local"]);
    assert!(
        skills
            .iter()
            .all(|skill| skill.enabled && skill.switchable())
    );

    let local = skills[1].clone();
    local.set_enabled(false).unwrap();
    assert!(!Harness::Pi.scan(&project)[1].enabled);
    let dir = scratch.project().join(".pi/skills/local");
    assert!(
        scratch
            .read("project/.pi/settings.json")
            .contains(&format!("-{}", dir.display()))
    );

    local.set_enabled(true).unwrap();
    assert!(Harness::Pi.scan(&project)[1].enabled);
    assert!(!scratch.read("project/.pi/settings.json").contains("skills"));
}

#[test]
fn opencode_switches_through_skill_permissions() {
    let scratch = Scratch::new("opencode-switch");
    scratch.skill(
        "project/.opencode/skills",
        "internal-docs",
        "---\nname: internal-docs\n---\n",
    );
    fs::create_dir_all(scratch.0.join("home/.config/opencode")).unwrap();
    fs::write(
        scratch.0.join("home/.config/opencode/opencode.json"),
        "{\"model\": \"x\", \"permission\": {\"skill\": {\"internal-*\": \"deny\"}}}",
    )
    .unwrap();
    let project = [scratch.project()];
    let skill = Harness::OpenCode.scan(&project).remove(0);
    assert!(!skill.enabled && skill.switchable());

    // On over a global pattern that denies it: the project allows it by name.
    skill.set_enabled(true).unwrap();
    assert!(Harness::OpenCode.scan(&project)[0].enabled);
    assert!(
        scratch
            .read("project/opencode.json")
            .contains("\"internal-docs\": \"allow\"")
    );

    skill.set_enabled(false).unwrap();
    assert!(!Harness::OpenCode.scan(&project)[0].enabled);
    assert!(
        scratch
            .read("project/opencode.json")
            .contains("\"internal-docs\": \"deny\"")
    );
}

#[test]
fn opencode_and_cursor_read_the_claude_and_agents_folders() {
    let scratch = Scratch::new("compat");
    scratch.skill(
        "home/.claude/skills",
        "claude-one",
        "---\nname: claude-one\n---\n",
    );
    scratch.skill("home/.agents/skills", "shared", "---\nname: shared\n---\n");
    scratch.skill("project/.opencode/skills", "oc", "---\nname: oc\n---\n");
    scratch.skill("project/.cursor/skills", "cur", "---\nname: cur\n---\n");
    let project = [scratch.project()];
    let names = |harness: Harness| -> Vec<String> {
        harness
            .scan(&project)
            .into_iter()
            .map(|skill| skill.name)
            .collect()
    };
    assert_eq!(names(Harness::OpenCode), ["claude-one", "oc", "shared"]);
    assert_eq!(names(Harness::Cursor), ["claude-one", "cur", "shared"]);
}
