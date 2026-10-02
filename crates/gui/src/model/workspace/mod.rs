//! What the app *is*, as opposed to what it draws: the projects that are
//! open, the sessions running in them, and the appearance the user picked.
//!
//! Views hold an `Entity<Workspace>` and read it; they never own a copy of any
//! of this. A mutation here notifies, and whichever views are observing repaint
//! — so nothing has to remember to tell the chrome that a session appeared.
//!
//! Everything [`crate::model::state`] persists lives here and nowhere else, which is
//! why [`Workspace::save`] can take no arguments.

#[cfg(feature = "desktop")]
use crate::{agent, model::update};
use crate::{
    data::{ColType, Column, Data, Edit, Page, Table},
    memory,
    model::{
        article::{self, Article},
        fonts,
        project::Project,
        session::ChatSession,
        settings::{self, Feature, Settings},
        state::{self, State},
        watch::{self, Watch},
    },
};
use artifact::board::Board;
use bezel::theme::AppExt as _;
use bezel::ui::AppExt as _;
use bezel::{
    gpui::{App, ClipboardItem, Context, EntityId, EventEmitter, Window},
    theme::{Brand, Tint, Vibrancy, appearance::AppearanceMode},
    ui::icons::Icon,
};
use cacp::schema::SessionConfigOptionValue;
use editor::AppExt as _;
use editor::Mode;
use futures::{StreamExt as _, channel::mpsc};
use markdown::AppExt as _;
use mcp::rail::{self, Change};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

/// The wait before the first retry of agent icons; each retry doubles it.
#[cfg(feature = "desktop")]
const ICON_RETRY_FIRST: std::time::Duration = std::time::Duration::from_secs(5);

/// How many times icons are retried after the first pass.
#[cfg(feature = "desktop")]
const ICON_RETRIES: u32 = 6;

// The rest of `impl Workspace`, split by what each part touches rather than
// left as one 1500-line block. Every one of them continues this module's
// scope — see the note at the head of each.
mod articles;
mod boards;
mod spaces;
pub use spaces::Showing;
mod order;
mod projects;
mod sessions;
mod tables;

/// What a table is called before it is named.
const UNTITLED: &str = "Untitled";

/// And a column.
const COLUMN: &str = "Column";

/// A project was re-read off disk and something a pane was showing has been
/// replaced — see [`crate::model::watch`].
///
/// What a view holds *about* an entry rather than the entry itself has to be
/// let go of here: a card's position is not that card's any more, and the
/// editor that had the caret is a different entity.
pub struct Reloaded;

/// The performance section's figures: what is in memory right now.
pub struct Resident {
    pub projects: usize,
    pub articles: usize,
    /// Articles holding an open [`editor::Editor`]. Built on first open and
    /// never dropped, so this only climbs.
    pub editors: usize,
    /// Covers decoded and held. Bounded — see [`crate::memory`].
    pub covers: usize,
    pub sessions: usize,
    /// Transcript entries across every one of them, read back whole at launch.
    pub items: usize,
}

/// Commands the palette remembers running.
const RECENT_COMMANDS: usize = 20;

pub struct Workspace {
    pub settings: Settings,
    pub projects: Vec<Project>,
    pub active: Option<usize>,
    pub appearance: AppearanceMode,
    pub cursor_blink: bool,
    /// Session ids are minted here and never reused, so a card's link to the
    /// session it opened stays unambiguous for the life of the process.
    next_id: u64,
    /// The body size the type ladder is scaled against, in points.
    pub text_size: f32,
    pub article_font_size: Option<f32>,
    /// What terminals and file views are set at before either is zoomed.
    pub mono_font_size: f32,
    /// The families the interface and the fixed-pitch surfaces are set in —
    /// see [`crate::model::fonts`].
    pub fonts: fonts::Families,
    /// The hue the greys carry, and how much of it.
    pub tint: Tint,
    /// How wide a page that has not been set either way is drawn — see
    /// [`crate::model::state::State::wide_pages`].
    pub wide_pages: bool,
    /// Whether an article shows its cover band when it has not said otherwise
    /// How a new board is laid out — see
    /// [`crate::model::settings::Appearance::board_view`].
    pub board_view: artifact::board::View,
    pub indent_project_rows: bool,
    /// Whether a long line in a code block wraps rather than scrolling — see
    /// [`apply_wrap_code`].
    pub wrap_code: bool,
    /// Whether the window is showing the frame meter. Runtime only — a switch
    /// you left on is not a preference worth restoring.
    pub meter: bool,
    /// The registry's mark for each configured agent, by name. Empty until the
    /// catalog lands; filled in by retries after an offline launch.
    agent_icons: HashMap<String, Icon>,
    /// Bumped by every [`Self::load_agent_icons`]; a retry loop holding an
    /// older value stops.
    #[cfg(feature = "desktop")]
    icon_pass: u64,
    /// What each project was last showing, by project path — where a launch
    /// puts you back.
    last: BTreeMap<PathBuf, state::Entry>,
    /// The hand-arranged order of each project's entries, by project path.
    /// Empty for a project nobody has dragged a row in, which lists by stamp
    /// until they do — see [`order`].
    pub(super) order: BTreeMap<PathBuf, Vec<state::Entry>>,
    /// The entries pinned to the top of each project's list, by project path
    /// — see [`order`].
    pub(super) pinned: BTreeMap<PathBuf, Vec<state::Entry>>,
    /// What each project's list is ordered by under its pins, by project path
    /// — see [`order`], which [`state::Sort::Manual`] reads and the other two
    /// leave alone.
    pub(super) sort: BTreeMap<PathBuf, state::Sort>,
    /// The main window's last frame, carried so the whole-file rewrite in
    /// [`Self::save`] keeps it.
    window: Option<state::Frame>,
    /// The arrangements this machine holds, and which one the window is
    /// showing. The window's rather than a project's: a space can hold panes
    /// from several — see [`spaces`].
    pub spaces: Vec<artifact::space::Space>,
    pub space: Option<usize>,
    /// Spaces whose members the sidebar hides, by id.
    folded_spaces: std::collections::HashSet<String>,
    /// The order the sidebar lists each space's members in, by space id.
    space_order: std::collections::BTreeMap<String, Vec<artifact::space::Member>>,
    /// The sidebar's sections whose rows are hidden, by name.
    folded_sections: std::collections::HashSet<String>,
    /// The palette's commands last run, most recent first.
    recent_commands: Vec<String>,
}

impl Workspace {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
        let projects: Vec<Project> = state
            .projects
            .into_iter()
            .map(|path| {
                let expanded = !state.collapsed.contains(&path);
                let mut project = Project::new(path);
                project.expanded = expanded;
                project
            })
            .collect();
        let active = (!projects.is_empty()).then_some(state.active);
        let restore: Vec<usize> = (0..projects.len()).collect();
        let look = settings.appearance.clone();
        cx.set_scrollbar_visibility(look.scrollbars.into());
        cx.set_editor_text_size(editor::TextSize {
            step: 1.,
            min: settings::CONTENT_TEXT_SIZE.0,
            max: settings::CONTENT_TEXT_SIZE.1,
        });
        crate::model::typography::set_terminal_size(look.mono_font_size, cx);
        crate::model::typography::set_file_size(look.mono_font_size, cx);
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: look.mode,
            cursor_blink: look.cursor_blink,
            text_size: look.text_size,
            article_font_size: look.article_font_size,
            mono_font_size: look.mono_font_size,
            fonts: fonts::families(),
            tint: Tint::new(look.hue, look.chroma),
            wide_pages: look.wide_pages,
            board_view: look.board_view,
            indent_project_rows: look.indent_project_rows,
            wrap_code: look.wrap_code,
            meter: false,
            next_id: 0,
            agent_icons: HashMap::new(),
            #[cfg(feature = "desktop")]
            icon_pass: 0,
            last: state.last,
            order: state.order,
            pinned: state.pinned,
            sort: state.sort,
            window: state.window,
            spaces: Self::in_order(crate::model::spaces::all(), &state.spaces),
            space: None,
            folded_spaces: state.folded_spaces.iter().cloned().collect(),
            space_order: state.space_order,
            folded_sections: state.folded_sections.iter().cloned().collect(),
            recent_commands: state.commands,
        };
        // The arrangement the window closed on, before any entry is opened:
        // `open_last_entry` is a project's answer and a space spans them.
        this.space = state
            .space
            .and_then(|id| this.spaces.iter().position(|space| space.id == id));
        for ix in restore {
            this.restore_sessions(ix);
            this.watch_project(ix, cx);
        }
        this.open_last_entry(cx);
        this.load_agent_icons(cx);
        this.refresh_door();
        cx.set_global(this.settings.browser.clone());
        this.take_rail(cx);
        // Temporary dev hook: `CYDONIA_TEST_PROMPT` sends a prompt on launch
        // so a turn can be verified without a composer. Here rather than on
        // connect, which a resume would fire again.
        if let Ok(prompt) = std::env::var("CYDONIA_TEST_PROMPT") {
            let id = this.active_id().or_else(|| {
                let entry = this.settings.agents.first().cloned()?;
                this.new_session(entry, None, cx)
            });
            if let Some(id) = id {
                this.send(id, prompt, cx);
            }
        }
        this
    }

    fn save(&self) {
        // The tools' copy of the rail, pushed wherever the list is written
        // down. A tool cannot read what is in memory here, and `state.toml` is
        // rewritten whole from this one place — so reading the file back would
        // be racing this line rather than avoiding it.
        rail::set_open(self.paths());
        state::save(&State {
            projects: self.paths(),
            active: self.active.unwrap_or_default(),
            collapsed: self
                .projects
                .iter()
                .filter(|project| !project.expanded)
                .map(|project| project.path.clone())
                .collect(),
            last: self.last.clone(),
            order: self.order.clone(),
            pinned: self.pinned.clone(),
            sort: self.sort.clone(),
            space: self.active_space().map(|space| space.id.clone()),
            spaces: self.spaces.iter().map(|space| space.id.clone()).collect(),
            folded_spaces: self
                .spaces
                .iter()
                .filter(|space| self.folded_spaces.contains(&space.id))
                .map(|space| space.id.clone())
                .collect(),
            space_order: self
                .space_order
                .iter()
                .filter(|(id, _)| self.spaces.iter().any(|space| &space.id == *id))
                .map(|(id, order)| (id.clone(), order.clone()))
                .collect(),
            folded_sections: {
                let mut folded: Vec<String> = self.folded_sections.iter().cloned().collect();
                folded.sort();
                folded
            },
            window: self.window,
            commands: self.recent_commands.clone(),
        });
    }

    /// The palette's commands last run, most recent first.
    pub fn recent_commands(&self) -> &[String] {
        &self.recent_commands
    }

    /// Put a command run from the palette at the head of the recent ones.
    pub fn ran_command(&mut self, key: String) {
        self.recent_commands.retain(|held| *held != key);
        self.recent_commands.insert(0, key);
        self.recent_commands.truncate(RECENT_COMMANDS);
        self.save();
    }

    /// Write down where the main window stands, if it has moved.
    pub fn set_window(&mut self, frame: state::Frame) {
        if self.window != Some(frame) {
            self.window = Some(frame);
            self.save();
        }
    }

    /// The other half of [`Self::save`]: the preferences, into the file a
    /// person edits. Split because the two move on different clocks — opening
    /// a project rewrites the bookkeeping and must not touch `settings.toml`,
    /// where somebody's comments live.
    fn save_appearance(&self) {
        let _ = settings::set_appearance(&settings::Appearance {
            mode: self.appearance,
            cursor_blink: self.cursor_blink,
            text_size: self.text_size,
            article_font_size: self.article_font_size,
            mono_font_size: self.mono_font_size,
            ui_font: self.fonts.sans.as_ref().map(ToString::to_string),
            article_font: self.fonts.body.as_ref().map(ToString::to_string),
            mono_font: self.fonts.mono.as_ref().map(ToString::to_string),
            hue: self.tint.hue,
            vibrancy: self.settings.appearance.vibrancy,
            blur: self.settings.appearance.blur,
            chroma: self.tint.chroma,
            wide_pages: self.wide_pages,
            board_view: self.board_view,
            indent_project_rows: self.indent_project_rows,
            settings_sidebar_fits: self.settings.appearance.settings_sidebar_fits,
            traffic_lights: self.settings.appearance.traffic_lights,
            scrollbars: self.settings.appearance.scrollbars,
            sidebar_scrollbars: self.settings.appearance.sidebar_scrollbars,
            wrap_code: self.wrap_code,
            highlight: self.settings.appearance.highlight,
            selection: self.settings.appearance.selection,
            search: self.settings.appearance.search,
            caret: self.settings.appearance.caret,
            caret_shape: self.settings.appearance.caret_shape,
        });
    }

    // ── agents ───────────────────────────────────────────────────────

    /// Fetch the catalog and keep each configured agent's icon. Off the UI
    /// thread — the registry is a blocking fetch on a cold cache — and a
    /// failure just leaves the map empty.
    fn load_agent_icons(&mut self, cx: &mut Context<Self>) {
        // No registry without the `desktop` feature: every agent is the
        // stand-in, and wears the shell.
        #[cfg(not(feature = "desktop"))]
        {
            let _ = cx;
            self.agent_icons = self
                .settings
                .agents
                .iter()
                .map(|agent| {
                    (
                        agent.name.clone(),
                        Icon::from(bezel::ui::icons::glyph::Shell),
                    )
                })
                .collect();
        }
        #[cfg(feature = "desktop")]
        let configured = self.settings.agents.clone();
        #[cfg(feature = "desktop")]
        let pass = {
            self.icon_pass += 1;
            self.icon_pass
        };
        // Retried with backoff while any configured agent is without a mark.
        // An entry the registry does not publish never gets one, so the
        // retries are capped rather than run until every entry has one.
        #[cfg(feature = "desktop")]
        cx.spawn(async move |this, cx| {
            let mut delay = ICON_RETRY_FIRST;
            for attempt in 0..=ICON_RETRIES {
                if attempt > 0 {
                    cx.background_executor().timer(delay).await;
                    delay *= 2;
                }
                let held = configured.clone();
                let icons = cx
                    .background_executor()
                    .spawn(async move { agent::icons(&held) })
                    .await;
                let complete = icons.len() == configured.len();
                let current = this
                    .update(cx, |workspace, cx| {
                        if workspace.icon_pass != pass {
                            return false;
                        }
                        // Merged: a pass that failed where an earlier one did
                        // not must not take a mark back off the screen.
                        workspace.agent_icons.extend(icons);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !current || complete {
                    return;
                }
            }
        })
        .detach();
    }

    /// The registry's mark for whatever this session runs on.
    pub fn agent_icon(&self, name: &str) -> Option<Icon> {
        self.agent_icons.get(name).cloned()
    }

    /// Which agent an unasked-for session runs on: whoever the project is
    /// already talking to, else the first one configured.
    ///
    /// Read out of `settings.toml` in both cases, never off the session. A
    /// session keeps its own copy of the entry it was opened on and the file
    /// moves under it — an agent uninstalled leaves a session pointing at a
    /// command that is gone, and handing that back opens a second session
    /// that cannot start either.
    pub fn preferred_agent(&self) -> Option<settings::Agent> {
        self.active_session()
            .and_then(|chat| self.agent_for(&chat.entry))
            .or_else(|| self.settings.agents.first())
            .cloned()
    }

    /// The configured agents as the tools offer them.
    pub(super) fn rail_agents(&self) -> Vec<rail::Agent> {
        self.settings
            .agents
            .iter()
            .map(|agent| rail::Agent {
                name: agent.name.clone(),
                id: agent.id.clone(),
            })
            .collect()
    }

    /// The entry `settings.toml` files for the agent this one is a copy of —
    /// see [`named`], and [`Self::restore_sessions`] for where the copy comes
    /// from.
    pub(super) fn agent_for(&self, held: &settings::Agent) -> Option<&settings::Agent> {
        named(&self.settings.agents, held.id.as_deref(), &held.name)
    }

    /// Re-read `settings.toml`. Installing an agent writes that file, and
    /// reading it back is what keeps this list identical to it — cheaper than
    /// a second copy of the rule for which entry an install replaces.
    pub fn reload_settings(&mut self, cx: &mut Context<Self>) {
        if let Ok(settings) = settings::load() {
            self.settings = settings;
            cx.set_global(self.settings.browser.clone());
            rail::set_agents(self.rail_agents());
            self.readopt_agents();
            self.load_agent_icons(cx);
        }
        cx.notify();
    }

    /// Hand every session the file's copy of the agent it names — see
    /// [`readopt`], which is the rule for one of them.
    fn readopt_agents(&mut self) {
        let agents = &self.settings.agents;
        for project in &mut self.projects {
            for chat in &mut project.sessions {
                readopt(agents, &mut chat.entry);
            }
        }
    }

    /// The settings window's choice. bezel repaints on `set_mode`;
    /// `settings.toml` is what makes it survive a relaunch.
    pub fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.appearance = mode;
        cx.set_appearance_mode(mode);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_vibrancy(&mut self, alpha: f32, cx: &mut Context<Self>) {
        let (min, max) = settings::VIBRANCY;
        self.settings.appearance.vibrancy = alpha.clamp(min, max);
        self.apply_transparency(cx);
    }

    pub fn set_blur(&mut self, radius: f32, cx: &mut Context<Self>) {
        let (min, max) = settings::BLUR;
        self.settings.appearance.blur = radius.clamp(min, max);
        self.apply_transparency(cx);
    }

    fn apply_transparency(&mut self, cx: &mut Context<Self>) {
        let look = &self.settings.appearance;
        apply_transparency(look.vibrancy, look.blur, cx);
        self.save_appearance();
        cx.notify();
    }

    /// Show a surface, or stop showing it. Written through to `settings.toml`
    /// rather than app state: it is the file the gate is read back from.
    /// A failed write leaves both halves alone, so the switch stays where it
    /// was rather than claiming a gate the file does not carry.
    ///
    /// Nothing is reloaded either way. What a project holds is read when it
    /// opens and the gate is applied at the accessors below, so switching one
    /// on shows what was already there rather than needing a rescan.
    pub fn set_feature(&mut self, feature: Feature, on: bool, cx: &mut Context<Self>) {
        if settings::set_feature(feature, on).is_err() {
            return;
        }
        feature.set(&mut self.settings.features, on);
        // `sessions` is half of what decides whether the door is open: the
        // only caller is an agent, and that switch is whether any run.
        self.refresh_door();
        cx.notify();
    }

    /// Move a command's chord, or take it back to its default.
    ///
    /// The file and this copy only. Putting the keymap back together is the
    /// caller's — see [`crate::view::keymap::rebind`], which the settings
    /// window runs once the write has landed: a model that reached into the
    /// keymap would be a model that knows what the app's actions are.
    pub fn set_shortcut(&mut self, key: &str, chord: Option<&str>, cx: &mut Context<Self>) {
        if settings::set_shortcut(key, chord).is_err() {
            return;
        }
        self.settings.shortcuts.set(key, chord);
        cx.notify();
    }

    pub fn set_emacs_shortcuts(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_emacs_shortcuts(on).is_err() {
            return;
        }
        self.settings.shortcuts.emacs = on;
        cx.notify();
    }

    // ── the tool server ──────────────────────────────────────────

    /// Open or close the door to match the two switches that decide it. One
    /// place, called from launch and from either of them.
    fn refresh_door(&self) {
        #[cfg(feature = "desktop")]
        agent::serve::serve(self.settings.mcp.serve && self.settings.features.sessions);
        #[cfg(feature = "desktop")]
        agent::serve::set_write(self.settings.mcp.write);
        #[cfg(feature = "desktop")]
        agent::serve::set_browser(
            self.settings.features.panel.browser && self.settings.browser.agents_read,
            self.settings.browser.agents_act,
        );
        #[cfg(feature = "desktop")]
        agent::serve::set_delete(self.settings.mcp.delete);
    }

    pub fn set_mcp_serve(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_mcp("serve", on).is_err() {
            return;
        }
        self.settings.mcp.serve = on;
        self.refresh_door();
        cx.notify();
    }

    /// Offer the tools that change a project, or withhold them. No rebind —
    /// what is offered is read off the switch on every call.
    pub fn set_mcp_write(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_mcp("write", on).is_err() {
            return;
        }
        self.settings.mcp.write = on;
        self.refresh_door();
        cx.notify();
    }

    /// Offer the tools that delete an entry, or withhold them.
    pub fn set_mcp_delete(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_mcp("delete", on).is_err() {
            return;
        }
        self.settings.mcp.delete = on;
        self.refresh_door();
        cx.notify();
    }

    // ── browser ──────────────────────────────────────────────────

    /// What a new browser tab opens on.
    pub fn set_browser_home(&mut self, home: String, cx: &mut Context<Self>) {
        if settings::set_browser("home", home.as_str()).is_err() {
            return;
        }
        self.settings.browser.home = home;
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Where a web link clicked in an article or a transcript opens.
    pub fn set_browser_links(&mut self, links: settings::Links, cx: &mut Context<Self>) {
        if settings::set_browser("links", links.key()).is_err() {
            return;
        }
        self.settings.browser.links = links;
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Offer agents the browser tools that read pages, or withhold them.
    pub fn set_browser_agents_read(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_browser("agents_read", on).is_err() {
            return;
        }
        self.settings.browser.agents_read = on;
        cx.set_global(self.settings.browser.clone());
        self.refresh_door();
        cx.notify();
    }

    /// Offer agents the browser tools that click and type, or withhold them.
    pub fn set_browser_agents_act(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_browser("agents_act", on).is_err() {
            return;
        }
        self.settings.browser.agents_act = on;
        cx.set_global(self.settings.browser.clone());
        self.refresh_door();
        cx.notify();
    }

    /// The hosts agents may not read or act on.
    pub fn set_browser_agents_blocked(&mut self, hosts: Vec<String>, cx: &mut Context<Self>) {
        let list: toml_edit::Array = hosts.iter().map(String::as_str).collect();
        if settings::set_browser("agents_blocked", list).is_err() {
            return;
        }
        self.settings.browser.agents_blocked = hosts;
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Whether pages built from now on keep what they store across restarts.
    pub fn set_browser_keep_signed_in(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_browser("keep_signed_in", on).is_err() {
            return;
        }
        self.settings.browser.keep_signed_in = on;
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Where the address field's searches go, as a `%s` template.
    pub fn set_browser_search(&mut self, search: String, cx: &mut Context<Self>) {
        if settings::set_browser("search", search.as_str()).is_err() {
            return;
        }
        self.settings.browser.search = search;
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Where the tools answer, while they do.
    pub fn mcp_url(&self) -> Option<String> {
        #[cfg(feature = "desktop")]
        return agent::serve::url();
        #[cfg(not(feature = "desktop"))]
        None
    }

    // ── releases ─────────────────────────────────────────────────

    /// Look for releases on our own, or stop looking. The file is what the next
    /// launch reads; the updater is what runs until then, so both are told.
    ///
    /// A release already staged is left alone: this switch is about the looking,
    /// and throwing away a bundle that is downloaded and verified would be a
    /// second thing under one name.
    /// Whether a finished turn is worth telling the system about. Written
    /// through to `settings.toml` first, as every switch here is.
    pub fn set_notify_turns(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_notify_turns(on).is_err() {
            return;
        }
        self.settings.notify_turns = on;
        cx.notify();
    }

    pub fn set_keep_pasted_images(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_keep_pasted_images(on).is_err() {
            return;
        }
        self.settings.keep_pasted_images = on;
        crate::model::media::set_pasting(self.settings.pasting(), cx);
        cx.notify();
    }

    pub fn set_paste_images_in_source(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_paste_images_in_source(on).is_err() {
            return;
        }
        self.settings.paste_images_in_source = on;
        crate::model::media::set_pasting(self.settings.pasting(), cx);
        cx.notify();
    }

    pub fn set_auto_update(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_auto_update(on).is_err() {
            return;
        }
        self.settings.auto_update = on;
        #[cfg(feature = "desktop")]
        if let Some(updater) = update::of(cx) {
            updater.update(cx, |updater, cx| updater.set_auto(on, cx));
        }
        cx.notify();
    }

    /// Move the cover ceiling. Written through to `settings.toml` first, for
    /// the reason [`Self::set_feature`] is, then applied to the cache
    /// that is already holding pictures under the old one.
    pub fn set_cover_memory(&mut self, mb: u64, window: &mut Window, cx: &mut Context<Self>) {
        if settings::set_cover_memory(mb).is_err() {
            return;
        }
        self.settings.cover_memory = mb;
        memory::covers(cx).update(cx, |covers, cx| {
            covers.set_limit(mb * 1_000_000, window, cx);
        });
        cx.notify();
    }

    /// Move the watch's bounce. Written through to `settings.toml` first, for
    /// the reason [`Self::set_cover_memory`] is, and clamped on the way in for
    /// the reason [`crate::model::watch::bounce`] clamps on the way out.
    ///
    /// Nothing is re-armed. The pump reads the interval on each pass, so the
    /// next event to land uses whatever this leaves behind.
    pub fn set_watch_bounce(&mut self, ms: u64, cx: &mut Context<Self>) {
        let ms = ms.clamp(watch::BOUNCE_RANGE.0, watch::BOUNCE_RANGE.1);
        if settings::set_watch_bounce(ms).is_err() {
            return;
        }
        self.settings.watch_bounce = ms;
        cx.notify();
    }

    /// The caret is bezel's, so the setting is: nothing here reads it back.
    pub fn set_cursor_blink(&mut self, blink: bool, cx: &mut Context<Self>) {
        self.cursor_blink = blink;
        cx.set_caret_blink(blink);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_traffic_lights(&mut self, on: bool, cx: &mut Context<Self>) {
        self.settings.appearance.traffic_lights = on;
        apply_caption_style(on, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_settings_sidebar_fits(&mut self, fits: bool, cx: &mut Context<Self>) {
        self.settings.appearance.settings_sidebar_fits = fits;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_caret_shape(&mut self, shape: settings::CaretShape, cx: &mut Context<Self>) {
        self.settings.appearance.caret_shape = shape;
        cx.set_caret_shape(shape.into());
        self.save_appearance();
        cx.notify();
    }

    pub fn set_text_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.text_size = points;
        cx.set_base_text_size(points);
        if self.article_font_size.is_none() {
            self.apply_article_font_size(cx);
        }
        self.save_appearance();
        cx.notify();
    }

    pub fn article_font_size(&self) -> f32 {
        self.article_font_size.unwrap_or(self.text_size)
    }

    fn apply_article_font_size(&self, cx: &mut Context<Self>) {
        let points = self.article_font_size();
        for editor in self
            .projects
            .iter()
            .flat_map(|project| &project.articles)
            .filter_map(|article| article.editor.as_ref())
        {
            editor.update(cx, |editor, cx| editor.set_text_size(points, cx));
        }
    }

    pub fn set_article_font_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.article_font_size = Some(settings::clamp_content_text_size(points));
        self.apply_article_font_size(cx);
        self.save_appearance();
        cx.notify();
    }

    /// The size every fixed-pitch surface starts at. Both are rebased: one
    /// saved size, and the zoom each of them carries is unwound against it.
    pub fn set_mono_font_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.mono_font_size = settings::clamp_content_text_size(points);
        crate::model::typography::set_terminal_size(self.mono_font_size, cx);
        crate::model::typography::set_file_size(self.mono_font_size, cx);
        self.save_appearance();
        cx.notify();
    }

    /// Set the interface family, the fixed-pitch one, or both. `None` in a
    /// slot is the palette's own face for it.
    pub fn set_fonts(&mut self, fonts: fonts::Families, cx: &mut Context<Self>) {
        self.fonts = fonts.clone();
        fonts::set(fonts, cx);
        self.save_appearance();
        cx.notify();
    }

    /// How wide a page with nothing of its own to say is set. Every open
    /// article redraws: the ones carrying a width of their own keep it, and
    /// the rest follow this.
    pub fn set_wide_pages(&mut self, wide: bool, cx: &mut Context<Self>) {
        self.wide_pages = wide;
        self.save_appearance();
        cx.notify();
    }

    /// How the next board made will be laid out. Nothing on screen moves: a
    /// board already made carries its own answer.
    pub fn set_default_board_view(&mut self, view: artifact::board::View, cx: &mut Context<Self>) {
        self.board_view = view;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_scrollbars(
        &mut self,
        value: settings::Scrollbars,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) {
        let look = &mut self.settings.appearance;
        if sidebar {
            look.sidebar_scrollbars = value;
        } else {
            look.scrollbars = value;
        }
        cx.set_scrollbar_visibility(look.scrollbars.into());
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_indent_project_rows(&mut self, indent: bool, cx: &mut Context<Self>) {
        self.indent_project_rows = indent;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_highlight(&mut self, value: settings::Highlight, cx: &mut Context<Self>) {
        self.settings.appearance.highlight = value;
        crate::view::article::set_highlight(value.color());
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_selection(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.selection = value;
        crate::model::fonts::set_selection(value, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_caret(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.caret = value;
        crate::model::fonts::set_caret(value, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_search(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.search = value;
        crate::view::article::set_search(value);
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_wrap_code(&mut self, wrap: bool, cx: &mut Context<Self>) {
        self.wrap_code = wrap;
        apply_wrap_code(wrap, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_tint(&mut self, tint: Tint, cx: &mut Context<Self>) {
        self.tint = tint;
        apply_tint(tint, cx);
        self.save_appearance();
        cx.notify();
    }

    // ── performance ──────────────────────────────────────────────────

    /// What this process is holding, counted off the state itself rather than
    /// tracked alongside it — a tally kept in parallel is a tally that can
    /// disagree with what is actually resident.
    pub fn resident(&self, cx: &App) -> Resident {
        let articles = || self.projects.iter().flat_map(|project| &project.articles);
        let sessions = || self.projects.iter().flat_map(|project| &project.sessions);
        Resident {
            projects: self.projects.len(),
            articles: articles().count(),
            editors: articles()
                .filter(|article| article.editor.is_some())
                .count(),
            covers: memory::covers(cx).read(cx).len(),
            sessions: sessions().count(),
            items: sessions().map(|chat| chat.items.len()).sum(),
        }
    }
}

impl EventEmitter<Reloaded> for Workspace {}

/// Point one session's entry at the file's copy of the agent it names.
///
/// A session carries its own copy, taken when it was opened, and one restored
/// while its agent was not on the machine carries the placeholder
/// [`Workspace::restore_sessions`] leaves in its place — a name and no command,
/// which is not [`ChatSession::resumable`]. Installing that agent writes the
/// entry the placeholder stood in for, so re-reading the file without this
/// leaves the session reading as stranded with the agent sitting right there
/// in Settings › Agents.
///
/// A session whose agent the file still does not name keeps what it has. That
/// is the one nothing here can help, and it is what the notice is for.
pub fn readopt(agents: &[settings::Agent], entry: &mut settings::Agent) {
    if let Some(found) = named(agents, entry.id.as_deref(), &entry.name) {
        *entry = found.clone();
    }
}

/// Find the agent `id` names, falling back to `name`.
///
/// The id is the registry's — `claude-acp` — and is the only stable half: a
/// display name is the publisher's to change, and one that changed used to
/// strand every session opened under the old one, with the agent sitting in
/// `settings.toml` under its new name and nothing matching it. The name is
/// still tried, for a session written before the id was stored and for an
/// entry somebody hand-wrote into the file, which carries no id at all.
pub fn named<'a>(
    agents: &'a [settings::Agent],
    id: Option<&str>,
    name: &str,
) -> Option<&'a settings::Agent> {
    id.and_then(|id| agents.iter().find(|agent| agent.id.as_deref() == Some(id)))
        .or_else(|| agents.iter().find(|agent| agent.name == name))
}

/// Point bezel's tint at the preference. Free rather than a method
/// because the window reads its background appearance while it is being opened,
/// which is before there is a workspace to ask.
pub fn apply_tint(tint: Tint, cx: &mut App) {
    cx.set_brand(Brand { tint, ..cx.brand() });
}

/// How a fence breaks its lines, handed to the renderer that paints one.
///
/// Every document at once, the article's and the transcript's alike: one
/// answer is installed for the app, the way the highlighter and the type
/// ladder are. There is nowhere narrower to put it — a fence is painted by
/// `markdown::render`, which takes no per-surface layout.
pub fn apply_wrap_code(wrap: bool, cx: &mut App) {
    cx.set_markdown_layout(markdown::Layout { wrap_code: wrap });
}

/// How bezel draws the window buttons off macOS.
pub fn apply_caption_style(traffic_lights: bool, cx: &mut App) {
    use bezel::ui::{AppExt as _, titlebar::CaptionStyle};
    cx.set_caption_style(match traffic_lights {
        true => CaptionStyle::Lights,
        false => CaptionStyle::Rectangular,
    });
}

/// Hand the answer to bezel, which reapplies it on every light/dark switch
/// from then on — including the one the OS makes at sunset, which reaches
/// nothing of ours.
///
/// Never [`Vibrancy::On`]: bezel's light palette carries no frosted tokens.
/// Off macOS the window is always opaque.
pub fn apply_transparency(vibrancy_alpha: f32, window_blur: f32, cx: &mut App) {
    cx.set_brand(Brand {
        vibrancy_alpha,
        window_blur,
        vibrancy: match cfg!(target_os = "macos") {
            true => Vibrancy::Auto,
            false => Vibrancy::Off,
        },
        ..cx.brand()
    });
}
