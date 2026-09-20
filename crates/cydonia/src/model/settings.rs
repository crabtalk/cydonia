//! Auto-generated settings — written with defaults on first run, read on
//! launch. Editable, but never requires user maintenance.

use crate::{memory, model::watch};
use anyhow::{Context, Result};
use bezel::theme::{TextStyle, appearance::AppearanceMode};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// What the file is called inside [`dir`].
const FILE: &str = "settings.toml";

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    /// The ceiling decoded covers run under, in megabytes. A bare key, so it
    /// is declared above `features`: one written after that table would belong
    /// to it.
    #[serde(default = "cover_memory")]
    pub cover_memory: u64,
    /// How long a project's `.cydonia/` has to go quiet before the watch
    /// re-reads it, in milliseconds — see [`crate::model::watch`]. Bare, so it
    /// belongs above `features` for the reason above.
    ///
    /// Read through [`watch::bounce`] and never used raw: this file is edited
    /// by hand, and the ends of the range are what a hand cannot reach past.
    #[serde(default = "watch_bounce")]
    pub watch_bounce: u64,
    /// Whether cydonia looks for a new release on its own. Bare, so it belongs
    /// above `features` for the reason above.
    ///
    /// What this switches is the looking and the fetching, not the swap: a
    /// staged release waits for the restart to be asked for. Off is an app
    /// that reaches the network for a version only when the menu item is
    /// picked — see [`crate::model::update`].
    #[serde(default = "auto_update")]
    pub auto_update: bool,
    /// Whether a turn finishing while the app is in the background is worth
    /// telling the system about. Bare, beside `auto_update`, for the reason
    /// above — and because the two are the same kind of thing: what the app
    /// does while nobody is looking at it.
    ///
    /// Only ever posted with the app in the background; a turn you watched
    /// finish is one you already know about — see
    /// [`crate::model::notify`].
    #[serde(default = "notify_turns")]
    pub notify_turns: bool,
    /// How the interface is painted. The first table, so the bare keys above
    /// keep belonging to the document rather than to it.
    #[serde(default)]
    pub appearance: Appearance,
    /// The chords the app answers to. A table, and empty in a fresh file —
    /// see [`Shortcuts`].
    #[serde(default)]
    pub shortcuts: Shortcuts,
    /// What the app will show. Every bare key has to go above it, and every
    /// table below — `[[agents]]` is the one that follows.
    #[serde(default)]
    pub features: Features,
    /// The tool server this app answers on. A table, so it sits between the
    /// two that are already here and never above a bare key.
    #[serde(default)]
    pub mcp: Mcp,
    #[serde(default)]
    pub agents: Vec<Agent>,
}

/// What the body size may be set to, in points: the ladder's smallest measured
/// role to Title3's, so bezel's fixed chrome heights hold at either end. Read
/// on the way in as well as by the control, because a size out of range paints
/// an interface nobody can read the settings window to fix.
pub const TEXT_SIZE: (f32, f32) = (11., 17.);

/// Content sizes have a wider range than interface chrome.
pub const CONTENT_TEXT_SIZE: (f32, f32) = (8., 40.);

pub fn clamp_content_text_size(points: f32) -> f32 {
    if points.is_finite() {
        points.clamp(CONTENT_TEXT_SIZE.0, CONTENT_TEXT_SIZE.1)
    } else {
        terminal::view::TERM_FONT_SIZE
    }
}

/// How the interface is painted — the reader's own answers, every one of them
/// a switch in Settings.
///
/// Here rather than in `state.toml` because these are preferences and not
/// bookkeeping: worth hand-editing, worth carrying to another machine, and
/// nothing to do with which projects happened to be open. `state.toml` keeps
/// what only this machine can answer — see [`crate::model::state`], and
/// [`crate::model::migrate`] for the move.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Light, dark, or whatever the OS is doing.
    pub mode: AppearanceMode,
    /// Whether the window is held opaque, and nothing at all for the person
    /// who has never said — the frost is then the appearance's own answer.
    /// See [`bezel::theme::Vibrancy`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opaque: Option<bool>,
    /// Whether the text caret blinks. Off holds it lit.
    pub cursor_blink: bool,
    /// The body size the type ladder is scaled against, in points. Clamped to
    /// [`TEXT_SIZE`] on the way in: this file is edited by hand, and a size
    /// out of range paints an interface nobody can read to fix it.
    pub text_size: f32,
    /// Unset keeps existing articles following the UI's base size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article_font_size: Option<f32>,
    /// The size fixed-pitch surfaces are set at: terminals, file source and
    /// previews. One size for both, the way [`Self::mono_font`] is one family
    /// for both. Each surface still zooms on its own — see
    /// [`crate::model::typography`].
    #[serde(alias = "terminal_font_size")]
    pub mono_font_size: f32,
    /// The family the interface is set in, as the system names it. Unset is
    /// the system UI font — see [`crate::model::fonts`], which resolves these
    /// and holds what a missing family falls back to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_font: Option<String>,
    /// The family prose is set in — articles, transcripts, anything rendered
    /// as a document. Unset follows [`Self::ui_font`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub article_font: Option<String>,
    /// The family for terminals, code and anything else set in a fixed pitch.
    /// Unset is the system's monospace face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mono_font: Option<String>,
    /// The greys' oklch hue in degrees, and how much of it they carry. Zero
    /// chroma is the shipped neutral, whatever the hue says.
    pub hue: f32,
    pub chroma: f32,
    /// How wide a page with nothing of its own to say is set. A page that
    /// *has* been decided about carries the decision in its own
    /// `properties.toml` and ignores this.
    pub wide_pages: bool,
    /// Whether an article shows the band over its first line. A page that
    /// *has* been decided about carries the decision in its own
    /// `properties.toml` and ignores this, the way [`Self::wide_pages`] works.
    ///
    /// On, which is how covers shipped.
    pub covers: bool,
    /// How a new board is laid out. Every board carries its own answer from the
    /// moment it is made — see [`artifact::board::Board::view`] — so this seeds
    /// one and never steers it afterwards.
    pub board_view: artifact::board::View,
    /// Indent sidebar items beneath project headings by one icon width.
    pub indent_project_rows: bool,
    pub scrollbars: Scrollbars,
    pub sidebar_scrollbars: Scrollbars,
    /// Whether a line too long for a code block wraps rather than scrolling
    /// sideways inside it — `markdown::Layout::wrap_code`.
    pub wrap_code: bool,
}

/// When overflowing panes show their scrollbars.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scrollbars {
    #[default]
    Scrolling,
    Always,
    Never,
}

impl From<Scrollbars> for bezel::ui::scroll::Visibility {
    fn from(value: Scrollbars) -> Self {
        match value {
            Scrollbars::Scrolling => Self::Scrolling,
            Scrollbars::Always => Self::Always,
            Scrollbars::Never => Self::Never,
        }
    }
}

impl Scrollbars {
    pub const ALL: [Self; 3] = [Self::Scrolling, Self::Always, Self::Never];

    pub fn label(self) -> &'static str {
        match self {
            Self::Scrolling => "While scrolling",
            Self::Always => "Always",
            Self::Never => "Never",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Scrolling => "scrolling",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: AppearanceMode::default(),
            opaque: None,
            cursor_blink: true,
            text_size: TextStyle::Body.size(),
            article_font_size: None,
            mono_font_size: terminal::view::TERM_FONT_SIZE,
            ui_font: None,
            article_font: None,
            mono_font: None,
            hue: 0.,
            chroma: 0.,
            wide_pages: false,
            covers: true,
            board_view: artifact::board::View::Lanes,
            indent_project_rows: true,
            scrollbars: Scrollbars::default(),
            sidebar_scrollbars: Scrollbars::Never,
            // Off, the way every code editor ships it: indentation is
            // structure, and wrapping loses the left column that makes nesting
            // scannable. Against bezel's own default, which wraps because
            // nothing scrolls a fence back to a caret typed off its right
            // edge — that is the cost, and the switch is the way back.
            wrap_code: false,
        }
    }
}

impl Appearance {
    /// Hand-edited sizes must remain finite and usable before layout sees them.
    pub fn normalize(&mut self) {
        self.text_size = if self.text_size.is_finite() {
            self.text_size.clamp(TEXT_SIZE.0, TEXT_SIZE.1)
        } else {
            Self::default().text_size
        };
        self.article_font_size = self.article_font_size.map(clamp_content_text_size);
        self.mono_font_size = clamp_content_text_size(self.mono_font_size);
        // A family hand-edited to the empty string names nothing; it is the
        // same answer as the key being absent.
        self.ui_font = self.ui_font.take().filter(|name| !name.trim().is_empty());
        self.article_font = self
            .article_font
            .take()
            .filter(|name| !name.trim().is_empty());
        self.mono_font = self.mono_font.take().filter(|name| !name.trim().is_empty());
    }
}

/// Text editing preferences and sparse command overrides.
/// Unknown command names are preserved for hand-edited settings.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Shortcuts {
    /// Add Option+B/F word movement and Option+D word deletion.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub emacs: bool,
    #[serde(flatten)]
    bindings: BTreeMap<String, String>,
}

impl Shortcuts {
    /// The one key here that is not a command: the chord the *system* holds,
    /// which brings the app forward from inside whatever else you are using.
    /// Absent means nothing is held — a key claimed across the whole desktop
    /// is one to be asked for, so a fresh install claims none.
    pub const ACTIVATE: &'static str = "activate";

    pub fn get(&self, key: &str) -> Option<&str> {
        self.bindings.get(key).map(String::as_str)
    }

    pub fn activate(&self) -> Option<&str> {
        self.get(Self::ACTIVATE)
    }

    /// Move one key in memory, to match what [`set_shortcut`] put in the file.
    /// `None` is back to the default, which is the key's absence.
    pub fn set(&mut self, key: &str, chord: Option<&str>) {
        match chord {
            Some(chord) => {
                self.bindings.insert(key.to_owned(), chord.to_owned());
            }
            None => {
                self.bindings.remove(key);
            }
        }
    }
}

/// Cydonia as an MCP server: the tools an agent reaches a project's boards
/// through.
///
/// On by default, and still opens nothing until `sessions` is on — the only
/// caller is an agent, and that switch is what decides whether any run. This
/// one is for saying no to the port while still running them.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Mcp {
    pub serve: bool,
    /// Whether the tools that change a project are offered at all. Off is a
    /// server an agent can read a board through and not touch it — the tools
    /// are left out of the list rather than refused on the call, because a
    /// tool an agent can see is one it will spend a turn trying.
    pub write: bool,
}

impl Default for Mcp {
    fn default() -> Self {
        Self {
            serve: true,
            write: false,
        }
    }
}

/// The surfaces a project can hold, minus articles — the one thing the app is
/// for, and so not something to be able to switch off.
///
/// Boards are on to begin with: a board is files in the project and nothing
/// runs to hold one, so a fresh install is a place to write and a place to
/// plan. The other two are off until they are asked for.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    /// Whether sessions may be opened. A session is the only thing that starts
    /// an agent, and an agent is a package this machine downloads and runs, so
    /// this is a gate over that as much as over the pane.
    pub sessions: bool,
    pub boards: bool,
    pub tables: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            sessions: false,
            boards: true,
            tables: false,
        }
    }
}

/// One switchable surface, named rather than reached as a field so the settings
/// section can list them and one writer can put any of them in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Sessions,
    Boards,
    Tables,
}

impl Feature {
    /// The order the Features section lists them in. Sessions first: it is the
    /// one that decides whether anything runs on this machine.
    pub const ALL: [Self; 3] = [Self::Sessions, Self::Boards, Self::Tables];

    /// The key it is written under, inside `[features]`.
    fn key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Boards => "boards",
            Self::Tables => "tables",
        }
    }

    pub fn on(self, features: &Features) -> bool {
        match self {
            Self::Sessions => features.sessions,
            Self::Boards => features.boards,
            Self::Tables => features.tables,
        }
    }

    pub fn set(self, features: &mut Features, on: bool) {
        match self {
            Self::Sessions => features.sessions = on,
            Self::Boards => features.boards = on,
            Self::Tables => features.tables = on,
        }
    }
}

/// One launchable ACP agent: `command args...` spawned over stdio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    /// The registry agent this was installed from, when it came from there.
    /// A hand-written entry has none, and is never touched by the installer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// What the cover ceiling is when the file does not say.
fn cover_memory() -> u64 {
    memory::DEFAULT_LIMIT / 1_000_000
}

/// And the watch's bounce, which the watch itself owns.
fn watch_bounce() -> u64 {
    watch::BOUNCE
}

/// Whether a fresh install looks for releases. On: an app that cannot tell you
/// it is out of date leaves you reading a changelog to find out.
fn auto_update() -> bool {
    true
}

/// Whether a fresh install says a turn has finished. On: it speaks only while
/// the app is in the background, which is the case it exists for.
fn notify_turns() -> bool {
    true
}

/// The launchers that resolve a package name on every run. An installed
/// agent's command is a path to an unpacked executable, which resolves nothing.
const RUNNERS: [&str; 3] = ["npx", "bunx", "pnpx"];

impl Agent {
    /// Whether every npm package this entry names carries an exact version.
    /// `npx pkg@latest` resolves against the registry on every launch, which is
    /// a different program each time.
    pub fn pinned(&self) -> bool {
        if !RUNNERS.contains(&self.command.as_str()) {
            return true;
        }
        self.args
            .iter()
            .filter(|arg| !arg.starts_with('-'))
            .all(|spec| {
                let name = cacp_agents::package_name(spec);
                spec.len() > name.len()
                    && spec[name.len() + 1..].starts_with(|c: char| c.is_ascii_digit())
            })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cover_memory: cover_memory(),
            watch_bounce: watch_bounce(),
            auto_update: auto_update(),
            notify_turns: notify_turns(),
            appearance: Appearance::default(),
            shortcuts: Shortcuts::default(),
            features: Features::default(),
            mcp: Mcp::default(),
            // None, and named by nobody but the person who put one here.
            //
            // A fresh install used to ship `npx` lines for claude and codex,
            // which claimed two integrations this machine had never been asked
            // for: the picker offered them, and the first one opened a session
            // that fetched a package off npm and ran it. Settings › Agents is
            // where an agent arrives — see [`crate::agent::install`], which
            // writes the entry — and until one does, this list is empty and
            // nothing here can spawn. [`crate::model::migrate::v0_1_4`] takes
            // the two lines back out of a file that already has them.
            agents: Vec::new(),
        }
    }
}

/// Where installed agents are put — `$XDG_DATA_HOME/cydonia`, defaulting to
/// `~/.local/share/cydonia`. Programs, not preferences, so they do not belong
/// beside the files a person edits.
pub fn data_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("cydonia"));
    }
    Ok(home()?.join(".local").join("share").join("cydonia"))
}

/// Cydonia's config directory: `$XDG_CONFIG_HOME/cydonia`, defaulting to
/// `~/.config/cydonia` — on macOS too, so a hand-edited settings.toml sits
/// where its neighbours do rather than in Application Support.
pub fn dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Ok(PathBuf::from(xdg).join("cydonia"));
    }
    Ok(home()?.join(".config").join("cydonia"))
}

/// The home every directory above hangs off — except under a test, where it is
/// a directory of this process's own.
///
/// A [`crate::model::workspace::Workspace`] writes `state.toml` whenever the
/// open projects change, and a test that builds one writes it too: run against
/// the real home, a suite replaces the project list of whoever ran it with a
/// list of temp directories (user report).
///
/// `NEXTEST` is set by the runner in every test process, which is what reaches
/// the integration binaries — they link this crate compiled without `cfg(test)`
/// and see none of it otherwise.
fn home() -> Result<PathBuf> {
    if cfg!(test) || std::env::var_os("NEXTEST").is_some() {
        return Ok(std::env::temp_dir().join(format!("cydonia-test-home-{}", std::process::id())));
    }
    dirs::home_dir().context("no home directory on this system")
}

/// `~/.config/cydonia/settings.toml`.
pub(crate) fn path() -> Result<PathBuf> {
    Ok(dir()?.join(FILE))
}

pub fn load() -> Result<Settings> {
    let dir = dir()?;
    let path = dir.join(FILE);
    if !path.exists() {
        let settings = Settings::default();
        std::fs::create_dir_all(&dir)?;
        let body = format!(
            "# generated by cydonia — edits are kept, deleting regenerates defaults\n\n{}",
            toml::to_string_pretty(&settings)?
        );
        std::fs::write(&path, body)?;
        return Ok(settings);
    }
    let content = std::fs::read_to_string(&path)?;
    let mut settings: Settings = toml::from_str(&content)
        .with_context(|| format!("invalid settings: {}", path.display()))?;
    // A floating tag is a different program on every launch. The line stays in
    // the file, where it can be read and fixed; it just never launches.
    settings.agents.retain(Agent::pinned);
    settings.appearance.normalize();
    Ok(settings)
}

/// Read `settings.toml`, hand it to `change`, and write it back when `change`
/// says there is something to write.
///
/// Edited with `toml_edit` rather than re-serialised: the file is meant to be
/// opened and changed by hand, and a round trip through a value tree would
/// silently delete every comment in it.
fn edit(change: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<bool>) -> Result<()> {
    let path = path()?;
    let body = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut =
        body.parse().context("settings.toml is not valid toml")?;
    if !change(&mut doc)? {
        return Ok(());
    }
    std::fs::write(&path, doc.to_string())?;
    Ok(())
}

/// One named table of the document, made if it is not there. Put in explicitly
/// rather than sprung from the index, because a table that arrives that way is
/// implicit and prints no header of its own.
fn table<'a>(doc: &'a mut toml_edit::DocumentMut, name: &str) -> Result<&'a mut toml_edit::Table> {
    let item = doc[name].or_insert(toml_edit::table());
    let Some(held) = item.as_table_mut() else {
        anyhow::bail!("`{name}` in settings.toml is not a table");
    };
    held.set_implicit(false);
    Ok(held)
}

/// Write the whole of `[appearance]`.
///
/// One call rather than a setter per key: the window holds these live and
/// any of them can move in a frame. Key by key through [`table`] all the same,
/// so a comment somebody wrote beside one of them survives the write.
pub fn set_appearance(appearance: &Appearance) -> Result<()> {
    edit(|doc| {
        write_appearance(doc, appearance)?;
        Ok(true)
    })
}

fn write_appearance(doc: &mut toml_edit::DocumentMut, appearance: &Appearance) -> Result<()> {
    let held = table(doc, "appearance")?;
    held["mode"] = toml_edit::value(match appearance.mode {
        AppearanceMode::System => "system",
        AppearanceMode::Light => "light",
        AppearanceMode::Dark => "dark",
    });
    // Never said is the absence of the key, not a `false` that would hand
    // this reader a frosted light mode they never asked for.
    match appearance.opaque {
        Some(opaque) => held["opaque"] = toml_edit::value(opaque),
        None => {
            held.remove("opaque");
        }
    }
    held["cursor_blink"] = toml_edit::value(appearance.cursor_blink);
    held["text_size"] = toml_edit::value(f64::from(appearance.text_size));
    match appearance.article_font_size {
        Some(size) => held["article_font_size"] = toml_edit::value(f64::from(size)),
        None => {
            held.remove("article_font_size");
        }
    }
    held["mono_font_size"] = toml_edit::value(f64::from(appearance.mono_font_size));
    // The two keys this one replaced. Left behind they would read as switches
    // that still do something.
    held.remove("terminal_font_size");
    held.remove("file_font_size");
    for (key, family) in [
        ("ui_font", &appearance.ui_font),
        ("article_font", &appearance.article_font),
        ("mono_font", &appearance.mono_font),
    ] {
        match family {
            Some(family) => held[key] = toml_edit::value(family.as_str()),
            None => {
                held.remove(key);
            }
        }
    }
    held["hue"] = toml_edit::value(f64::from(appearance.hue));
    held["chroma"] = toml_edit::value(f64::from(appearance.chroma));
    held["wide_pages"] = toml_edit::value(appearance.wide_pages);
    held["covers"] = toml_edit::value(appearance.covers);
    held["board_view"] = toml_edit::value(appearance.board_view.key());
    held["indent_project_rows"] = toml_edit::value(appearance.indent_project_rows);
    held["scrollbars"] = toml_edit::value(appearance.scrollbars.key());
    held["sidebar_scrollbars"] = toml_edit::value(appearance.sidebar_scrollbars.key());
    held["wrap_code"] = toml_edit::value(appearance.wrap_code);
    Ok(())
}

/// Switch a feature on or off in the file.
pub fn set_feature(feature: Feature, on: bool) -> Result<()> {
    edit(|doc| {
        table(doc, "features")?[feature.key()] = toml_edit::value(on);
        Ok(true)
    })
}

/// Write one chord into `[shortcuts]`, or take it back out.
///
/// `None` removes the key rather than writing a default in its place: the
/// table says what differs, and a command sitting on its default differs in
/// nothing.
pub fn set_shortcut(key: &str, chord: Option<&str>) -> Result<()> {
    edit(|doc| {
        let held = table(doc, "shortcuts")?;
        match chord {
            Some(chord) => held[key] = toml_edit::value(chord),
            None => {
                held.remove(key);
            }
        }
        Ok(true)
    })
}

/// Enable Emacs word shortcuts without replacing the platform defaults.
pub fn set_emacs_shortcuts(on: bool) -> Result<()> {
    edit(|doc| {
        let held = table(doc, "shortcuts")?;
        if on {
            held["emacs"] = toml_edit::value(true);
        } else {
            held.remove("emacs");
        }
        Ok(true)
    })
}

/// Write one key of `[mcp]`.
pub fn set_mcp(key: &str, on: bool) -> Result<()> {
    edit(|doc| {
        table(doc, "mcp")?[key] = toml_edit::value(on);
        Ok(true)
    })
}

/// Move the cover ceiling in the file, in megabytes.
pub fn set_cover_memory(mb: u64) -> Result<()> {
    edit(|doc| {
        doc["cover_memory"] = toml_edit::value(mb as i64);
        Ok(true)
    })
}

/// Move the watch's bounce in the file, in milliseconds.
pub fn set_watch_bounce(ms: u64) -> Result<()> {
    edit(|doc| {
        doc["watch_bounce"] = toml_edit::value(ms as i64);
        Ok(true)
    })
}

/// Switch the release check on or off in the file.
pub fn set_auto_update(on: bool) -> Result<()> {
    edit(|doc| {
        doc["auto_update"] = toml_edit::value(on);
        Ok(true)
    })
}

/// Switch the finished-turn notification on or off in the file.
pub fn set_notify_turns(on: bool) -> Result<()> {
    edit(|doc| {
        doc["notify_turns"] = toml_edit::value(on);
        Ok(true)
    })
}

/// Put `agent` in the file, replacing whichever entry already launches it.
///
/// `supersedes` is the npm package the agent is published as, which is how an
/// install claims the hand-written `@latest` entry that shipped as a default
/// instead of sitting next to it. A replaced entry keeps its own `name`: the
/// person who wrote it chose that, and only the command underneath has moved.
pub fn put_agent(agent: &Agent, supersedes: Option<&str>) -> Result<()> {
    edit(|doc| {
        let agents = doc["agents"].or_insert(toml_edit::Item::ArrayOfTables(
            toml_edit::ArrayOfTables::new(),
        ));
        let Some(agents) = agents.as_array_of_tables_mut() else {
            anyhow::bail!("`agents` in settings.toml is not a list of tables");
        };
        let existing = agents
            .iter()
            .position(|table| claims(table, agent, supersedes));
        let name = existing
            .and_then(|ix| agents.get(ix))
            .and_then(|table| table.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or(&agent.name)
            .to_owned();
        // Whatever preceded the entry — the file's header, a note the user left
        // above it — is trivia hanging off the table, and replacing the table
        // throws it away unless it is carried across by hand.
        let decor = existing
            .and_then(|ix| agents.get(ix))
            .map(|table| table.decor().clone());

        let mut entry = toml_edit::Table::new();
        entry["name"] = toml_edit::value(name);
        if let Some(id) = &agent.id {
            entry["id"] = toml_edit::value(id.clone());
        }
        entry["command"] = toml_edit::value(agent.command.clone());
        let mut args = toml_edit::Array::new();
        for arg in &agent.args {
            args.push(arg.as_str());
        }
        entry["args"] = toml_edit::value(args);
        if !agent.env.is_empty() {
            let mut env = toml_edit::InlineTable::new();
            for (key, value) in &agent.env {
                env.insert(key, value.as_str().into());
            }
            entry["env"] = toml_edit::value(env);
        }

        match existing {
            Some(ix) => {
                if let Some(decor) = decor {
                    *entry.decor_mut() = decor;
                }
                *agents.get_mut(ix).expect("position is in range") = entry;
            }
            None => agents.push(entry),
        }
        Ok(true)
    })
}

/// Drop the entry installed from registry agent `id`.
pub fn remove_agent(id: &str) -> Result<()> {
    edit(|doc| {
        let Some(agents) = doc
            .get_mut("agents")
            .and_then(|a| a.as_array_of_tables_mut())
        else {
            return Ok(false);
        };
        let Some(ix) = agents
            .iter()
            .position(|table| table.get("id").and_then(|i| i.as_str()) == Some(id))
        else {
            return Ok(false);
        };
        // The file's header hangs off whichever entry comes first. If that is the
        // one being dropped, the header has to move down onto its successor or it
        // leaves with it.
        let prefix = agents
            .get(ix)
            .and_then(|table| table.decor().prefix().cloned());
        agents.remove(ix);
        if ix == 0
            && let Some(prefix) = prefix
        {
            match agents.get_mut(0) {
                Some(first) => first.decor_mut().set_prefix(prefix),
                None => doc.as_table_mut().decor_mut().set_prefix(prefix),
            }
        }
        Ok(true)
    })
}

/// Whether an existing entry is the one this install replaces: the same
/// registry agent, or a launcher for the same npm package.
fn claims(table: &toml_edit::Table, agent: &Agent, supersedes: Option<&str>) -> bool {
    let field = |key| table.get(key).and_then(|v| v.as_str());
    if agent.id.is_some() && field("id") == agent.id.as_deref() {
        return true;
    }
    let Some(package) = supersedes else {
        return false;
    };
    table
        .get("args")
        .and_then(|args| args.as_array())
        .is_some_and(|args| {
            args.iter()
                .filter_map(|v| v.as_str())
                .any(|arg| cacp_agents::package_name(arg) == package)
        })
}

#[cfg(test)]
#[path = "../../tests/unit/settings_typography.rs"]
mod typography_tests;
