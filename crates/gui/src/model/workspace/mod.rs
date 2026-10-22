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
    model::{
        article::{self, Article},
        fonts,
        project::{Project, ProjectId, ProjectIds},
        session::ChatSession,
        settings::{self, Feature, Settings},
        state::{self, State},
        watch::{self, Watch},
    },
};
use artifact::board::Board;
use bezel::{
    gpui::{App, AppContext as _, Context, EntityId, EventEmitter},
    theme::{AppExt as _, Brand, Tint, Vibrancy, appearance::AppearanceMode},
    ui::{AppExt as _, icons::Icon},
};
use cydonia_acp::proto::SessionConfigOptionValue;
use editor::AppExt as _;
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
mod apps;
mod articles;
mod boards;
mod labels;
mod spaces;
mod tabs;
pub use sessions::ArticleChange;
pub use spaces::{Entry, Member, Showing, Space};
pub use tabs::{Opening, Slot};
mod order;
mod projects;
pub mod references;
mod sessions;
mod tables;

/// What a table is called before it is named.
const UNTITLED: &str = "Untitled";

/// And a column.
const COLUMN: &str = "Column";

/// Commands the palette remembers running.
const RECENT_COMMANDS: usize = 20;

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
    /// Articles holding an open [`editor::Document`].
    pub documents: usize,
    /// Sessions whose transcript is in memory. A closed one's is dropped once
    /// its file matches, and read back when it is opened.
    pub sessions: usize,
    /// Transcript entries across those sessions.
    pub items: usize,
}

pub struct Workspace {
    pub settings: Settings,
    pub projects: Vec<Project>,
    pub active: Option<usize>,
    /// Every project path seen, open or not, and its id.
    ids: ProjectIds,
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
    /// [`crate::model::settings::Appearance::wide_pages`].
    pub wide_pages: bool,
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
    /// What each project was last showing — where a launch puts you back.
    last: BTreeMap<ProjectId, state::Entry>,
    /// The hand-arranged order of each project's entries. Empty for a project
    /// nobody has dragged a row in, which lists by stamp until they do — see
    /// [`order`].
    pub(super) order: BTreeMap<ProjectId, Vec<state::Entry>>,
    /// The entries pinned to the top of each project's list — see [`order`].
    pub(super) pinned: BTreeMap<ProjectId, Vec<state::Entry>>,
    /// What each project's list is ordered by under its pins — see [`order`],
    /// which [`state::Sort::Manual`] reads and the other two leave alone.
    pub(super) sort: BTreeMap<ProjectId, state::Sort>,
    /// The main window's last frame, carried so the whole-file rewrite in
    /// [`Self::save`] keeps it.
    window: Option<state::Frame>,
    /// The arrangements this machine holds. The window's rather than a
    /// project's: a space can hold panes from several — see [`spaces`]. Which
    /// one is shown is the strip's `front`.
    pub spaces: Vec<Space>,
    /// The window's tabs, and which is in front — see [`tabs`].
    strip: Vec<Slot>,
    front: Option<Slot>,
    /// The tab the next opening replaces — see [`Workspace::open_tab`].
    /// Runtime only.
    preview: Option<Member>,
    /// Spaces whose members the sidebar hides, by id.
    folded_spaces: std::collections::HashSet<String>,
    /// The order the sidebar lists each space's members in, by space id.
    space_order: std::collections::BTreeMap<String, Vec<Member>>,
    /// The sidebar's sections whose rows are hidden, by name.
    folded_sections: std::collections::HashSet<String>,
    /// The palette's commands last run, most recent first.
    recent_commands: Vec<String>,
    /// The release whose What's New was last shown — [`State::release`].
    release: Option<crate::model::version::Version>,
    /// Every app on this machine, by id — see [`apps`].
    pub apps: Vec<crate::model::apps::HeldApp>,
    /// The watch on the apps' records and loaded folders.
    apps_watch: Option<crate::model::watch::Watch>,
}

impl Workspace {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
        // The open projects first, so each id's path is the one its project
        // is listed under.
        let mut ids = ProjectIds::default();
        let projects: Vec<Project> = state
            .projects
            .into_iter()
            .map(|path| {
                let expanded = !state.collapsed.contains(&path);
                let mut project = Project::new(ids.intern(&path), path, cx);
                project.expanded = expanded;
                project
            })
            .collect();
        let last = ids.intern_keys(state.last);
        let order = ids.intern_keys(state.order);
        let pinned = ids.intern_keys(state.pinned);
        let sort = ids.intern_keys(state.sort);
        let spaces = Self::in_order(
            <crate::model::spaces::Shelf as crate::model::spaces::Store>::all()
                .into_iter()
                .map(|space| space.map(&mut |path| ids.intern(&path)))
                .collect(),
            &state.spaces,
        );
        let space_order = state
            .space_order
            .into_iter()
            .map(|(space, members)| {
                let members = members
                    .into_iter()
                    .map(|member| member.map(|path| ids.intern(&path)))
                    .collect();
                (space, members)
            })
            .collect();
        let tabs: Option<Vec<Slot>> = state.tabs.map(|tabs| {
            tabs.into_iter()
                .map(|slot| slot.map(|path| ids.intern(&path)))
                .collect()
        });
        let front = state.front.map(|slot| slot.map(|path| ids.intern(&path)));
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
        crate::model::session_preferences::OfferCache::load(cx);
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: look.mode,
            cursor_blink: look.cursor_blink,
            text_size: look.text_size,
            article_font_size: look.article_font_size,
            mono_font_size: look.mono_font_size,
            fonts: fonts::Families::of(&look),
            tint: Tint::new(look.hue, look.chroma),
            wide_pages: look.wide_pages,
            board_view: look.board_view,
            indent_project_rows: look.indent_project_rows,
            wrap_code: look.wrap_code,
            meter: false,
            ids,
            next_id: 0,
            agent_icons: HashMap::new(),
            #[cfg(feature = "desktop")]
            icon_pass: 0,
            last,
            order,
            pinned,
            sort,
            window: state.window,
            spaces,
            strip: Vec::new(),
            front: None,
            preview: None,
            folded_spaces: state.folded_spaces.iter().cloned().collect(),
            space_order,
            folded_sections: state.folded_sections.iter().cloned().collect(),
            recent_commands: state.commands,
            release: state.release,
            apps: Vec::new(),
            apps_watch: None,
        };
        this.app_recorded(cx);
        for ix in restore {
            this.restore_sessions(ix, cx);
            this.watch_project(ix, cx);
        }
        this.open_last_entry(cx);
        this.restore_tabs(tabs, front, state.space, cx);
        this.bring_up_tabs(cx);
        this.bring_up_space(cx);
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
            last: self.ids.path_keys(&self.last),
            order: self.ids.path_keys(&self.order),
            pinned: self.ids.path_keys(&self.pinned),
            sort: self.ids.path_keys(&self.sort),
            space: self.active_space().map(|space| space.id.clone()),
            spaces: self.space_tabs().map(str::to_owned).collect(),
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
                .map(|(id, order)| {
                    let order = order
                        .iter()
                        .map(|member| member.clone().map(|id| self.path_of(id).to_path_buf()))
                        .collect();
                    (id.clone(), order)
                })
                .collect(),
            folded_sections: {
                let mut folded: Vec<String> = self.folded_sections.iter().cloned().collect();
                folded.sort();
                folded
            },
            window: self.window,
            commands: self.recent_commands.clone(),
            release: self.release.clone(),
            tabs: Some(
                self.strip
                    .iter()
                    .map(|slot| slot.clone().map(|id| self.path_of(id).to_path_buf()))
                    .collect(),
            ),
            front: self
                .front_slot()
                .map(|slot| slot.clone().map(|id| self.path_of(id).to_path_buf())),
        });
    }

    /// Whether `notes`, a release's What's New, are newer than any shown.
    pub fn release_unseen(&self, notes: &crate::model::version::Version) -> bool {
        self.release
            .as_ref()
            .is_none_or(|seen| notes.newer_than(seen))
    }

    /// Record `notes` as shown.
    pub fn saw_release(&mut self, notes: crate::model::version::Version) {
        if self.release_unseen(&notes) {
            self.release = Some(notes);
            self.save();
        }
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
            theme: self.settings.appearance.theme.clone(),
            hue: self.tint.hue,
            vibrancy: self.settings.appearance.vibrancy,
            blur: self.settings.appearance.blur,
            chroma: self.tint.chroma,
            wide_pages: self.wide_pages,
            outline: self.settings.appearance.outline,
            comments: self.settings.appearance.comments,
            board_view: self.board_view,
            indent_project_rows: self.indent_project_rows,
            extended_sidebar: self.settings.appearance.extended_sidebar,
            spaces_in_sidebar: self.settings.appearance.spaces_in_sidebar,
            floating: self.settings.appearance.floating,
            content_opacity: self.settings.appearance.content_opacity,
            settings_sidebar_fits: self.settings.appearance.settings_sidebar_fits,
            traffic_lights: self.settings.appearance.traffic_lights,
            scrollbars: self.settings.appearance.scrollbars,
            sidebar_scrollbars: self.settings.appearance.sidebar_scrollbars,
            wrap_code: self.wrap_code,
            highlight: self.settings.appearance.highlight,
            selection: self.settings.appearance.selection,
            search: self.settings.appearance.search,
            comment_color: self.settings.appearance.comment_color,
            caret: self.settings.appearance.caret,
            caret_shape: self.settings.appearance.caret_shape,
            caret_height: self.settings.appearance.caret_height,
            hollow_caret: self.settings.appearance.hollow_caret,
            terminal_caret: self.settings.appearance.terminal_caret,
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
    pub fn preferred_agent(&self, cx: &App) -> Option<settings::Agent> {
        self.active_session(cx)
            .and_then(|chat| self.agent_for(&chat.entry))
            .or_else(|| self.settings.agents.first())
            .cloned()
    }

    /// The agent at `at` in the settings, or the preferred one where `at` is
    /// `None` — what a "New session" choice names.
    pub fn agent_at(&self, at: Option<usize>, cx: &App) -> Option<settings::Agent> {
        match at {
            Some(at) => self.settings.agents.get(at).cloned(),
            None => self.preferred_agent(cx),
        }
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
            self.readopt_agents(cx);
            self.load_agent_icons(cx);
        }
        cx.notify();
    }

    /// Hand every session the file's copy of the agent it names — see
    /// [`readopt`], which is the rule for one of them.
    fn readopt_agents(&self, cx: &mut App) {
        let agents = &self.settings.agents;
        for project in &self.projects {
            for held in &project.sessions {
                held.chat
                    .update(cx, |chat, _| readopt(agents, &mut chat.entry));
            }
        }
    }

    /// Paint `appearance` in a bundled theme family, or bezel's own palette
    /// for `None`.
    pub fn set_theme(
        &mut self,
        appearance: bezel::theme::Appearance,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.settings.appearance.theme.set(appearance, name);
        self.apply_palette(cx);
        self.save_appearance();
        cx.notify();
    }

    /// The family `appearance` is painted in, if it names one with a variant
    /// for it.
    pub fn theme(
        &self,
        appearance: bezel::theme::Appearance,
    ) -> Option<&'static bezel::theme::ThemeFamily> {
        self.settings
            .appearance
            .theme
            .get(appearance)
            .and_then(crate::model::themes::named)
            .filter(|family| family.variant(appearance).is_some())
    }

    /// Rebuild the palette from the appearance settings and the families.
    fn apply_palette(&self, cx: &mut App) {
        crate::model::palette::apply(
            crate::model::palette::Inputs::of(&self.settings.appearance, self.fonts.clone()),
            cx,
        );
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

    pub fn set_content_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        let (min, max) = settings::CONTENT_OPACITY;
        self.settings.appearance.content_opacity = opacity.clamp(min, max);
        self.apply_transparency(cx);
    }

    fn apply_transparency(&mut self, cx: &mut Context<Self>) {
        apply_transparency(&self.settings.appearance, cx);
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
        if self.settings.mcp.set_serve(on).is_err() {
            return;
        }
        self.refresh_door();
        cx.notify();
    }

    /// Offer the tools that change a project, or withhold them. No rebind —
    /// what is offered is read off the switch on every call.
    pub fn set_mcp_write(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.settings.mcp.set_write(on).is_err() {
            return;
        }
        self.refresh_door();
        cx.notify();
    }

    /// Offer the tools that delete an entry, or withhold them.
    pub fn set_mcp_delete(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.settings.mcp.set_delete(on).is_err() {
            return;
        }
        self.refresh_door();
        cx.notify();
    }

    // ── browser ──────────────────────────────────────────────────

    /// What a new browser tab opens on.
    pub fn set_browser_home(&mut self, home: String, cx: &mut Context<Self>) {
        if self.settings.browser.set_home(home).is_err() {
            return;
        }
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Offer agents the browser tools that read pages, or withhold them.
    pub fn set_browser_agents_read(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.settings.browser.set_agents_read(on).is_err() {
            return;
        }
        cx.set_global(self.settings.browser.clone());
        self.refresh_door();
        cx.notify();
    }

    /// Offer agents the browser tools that click and type, or withhold them.
    pub fn set_browser_agents_act(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.settings.browser.set_agents_act(on).is_err() {
            return;
        }
        cx.set_global(self.settings.browser.clone());
        self.refresh_door();
        cx.notify();
    }

    /// The hosts agents may not read or act on.
    pub fn set_browser_agents_blocked(&mut self, hosts: Vec<String>, cx: &mut Context<Self>) {
        if self.settings.browser.set_agents_blocked(hosts).is_err() {
            return;
        }
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Whether pages built from now on keep what they store across restarts.
    pub fn set_browser_keep_signed_in(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.settings.browser.set_keep_signed_in(on).is_err() {
            return;
        }
        cx.set_global(self.settings.browser.clone());
        cx.notify();
    }

    /// Where the address field's searches go, as a `%s` template.
    pub fn set_browser_search(&mut self, search: String, cx: &mut Context<Self>) {
        if self.settings.browser.set_search(search).is_err() {
            return;
        }
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

    /// Whether a finished turn is worth telling the system about. Written
    /// through to `settings.toml` first, as every switch here is.
    pub fn set_notify_turns(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_notify_turns(on).is_err() {
            return;
        }
        self.settings.notify_turns = on;
        cx.notify();
    }

    pub fn set_drawer_composer(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_drawer_composer(on).is_err() {
            return;
        }
        self.settings.drawer_composer = on;
        cx.notify();
    }

    pub fn set_download_web_images(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_download_web_images(on).is_err() {
            return;
        }
        self.settings.download_web_images = on;
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

    /// Look for releases on our own, or stop looking. Written to the file the
    /// next launch reads and told to the running updater. A release already
    /// staged is left alone.
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

    /// Move the watch's bounce. Written through to `settings.toml` first, for
    /// the reason [`Self::set_feature`] is, and clamped on the way in for
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

    pub fn set_floating(&mut self, floating: bool, cx: &mut Context<Self>) {
        self.settings.appearance.floating = floating;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_extended_sidebar(&mut self, extended: bool, cx: &mut Context<Self>) {
        self.settings.appearance.extended_sidebar = extended;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_spaces_in_sidebar(&mut self, on: bool, cx: &mut Context<Self>) {
        self.settings.appearance.spaces_in_sidebar = on;
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

    pub fn set_terminal_caret(&mut self, on: bool, cx: &mut Context<Self>) {
        self.settings.appearance.terminal_caret = on;
        fonts::set_terminal_caret(on);
        self.apply_palette(cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_caret_height(&mut self, height: settings::CaretHeight, cx: &mut Context<Self>) {
        self.settings.appearance.caret_height = height;
        cx.set_caret_height(height.into());
        self.save_appearance();
        cx.notify();
    }

    pub fn set_hollow_caret(&mut self, on: bool, cx: &mut Context<Self>) {
        self.settings.appearance.hollow_caret = on;
        cx.set_inactive_caret(settings::inactive_caret(on));
        self.save_appearance();
        cx.notify();
    }

    pub fn set_text_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.text_size = points;
        cx.set_base_text_size(points);
        self.save_appearance();
        cx.notify();
    }

    pub fn article_font_size(&self) -> f32 {
        self.article_font_size.unwrap_or(self.text_size)
    }

    pub fn set_article_font_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.article_font_size = Some(settings::clamp_content_text_size(points));
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
        self.fonts = fonts;
        self.apply_palette(cx);
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

    pub fn set_comments(&mut self, comments: settings::CommentDisplay, cx: &mut Context<Self>) {
        self.settings.appearance.comments = comments;
        self.save_appearance();
        cx.notify();
    }

    pub fn set_outline(&mut self, on: bool, cx: &mut Context<Self>) {
        self.settings.appearance.outline = on;
        self.save_appearance();
        cx.notify();
    }

    /// How the next board made will be laid out. Nothing on screen moves: a
    /// board already made carries its own answer.
    pub fn set_default_board_view(&mut self, view: artifact::board::View, cx: &mut Context<Self>) {
        self.board_view = view;
        rail::set_board_view(view);
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

    pub fn set_highlight(&mut self, value: settings::Paint, cx: &mut Context<Self>) {
        self.settings.appearance.highlight = value;
        crate::model::palette::Washes::of(&self.settings.appearance).install();
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_selection(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.selection = value;
        self.apply_palette(cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_caret(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.caret = value;
        self.apply_palette(cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_search(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.search = value;
        crate::model::palette::Washes::of(&self.settings.appearance).install();
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_comment_color(&mut self, value: Option<settings::Paint>, cx: &mut Context<Self>) {
        self.settings.appearance.comment_color = value;
        crate::model::palette::Washes::of(&self.settings.appearance).install();
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
        let sessions = || self.chats(cx).filter(|chat| chat.history_loaded());
        Resident {
            projects: self.projects.len(),
            articles: articles().count(),
            documents: articles()
                .filter(|article| article.document.is_some())
                .count(),
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

/// Point bezel's tint at the preference. Free rather than a method because the
/// window reads its background appearance while it is being opened, which is
/// before there is a workspace to ask.
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
/// Never [`Vibrancy::On`]: bezel's light palette carries no translucent tokens.
/// Off macOS the window is always opaque.
pub fn apply_transparency(look: &settings::Appearance, cx: &mut App) {
    cx.set_brand(Brand {
        vibrancy_alpha: look.vibrancy,
        window_blur: look.blur,
        content_opacity: look.content_opacity,
        vibrancy: match cfg!(target_os = "macos") {
            true => Vibrancy::Auto,
            false => Vibrancy::Off,
        },
        ..cx.brand()
    });
}
