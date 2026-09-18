//! What the app *is*, as opposed to what it draws: the projects that are
//! open, the sessions running in them, and the appearance the user picked.
//!
//! Views hold an `Entity<Workspace>` and read it; they never own a copy of any
//! of this. A mutation here notifies, and whichever views are observing repaint
//! — so nothing has to remember to tell the chrome that a session appeared.
//!
//! Everything [`crate::model::state`] persists lives here and nowhere else, which is
//! why [`Workspace::save`] can take no arguments.

use crate::{
    agent,
    data::{ColType, Column, Data, Edit, Page, Table},
    memory,
    model::{
        article::{self, Article},
        project::Project,
        session::ChatSession,
        settings::{self, Feature, Settings},
        state::{self, State},
        update,
        watch::{self, Watch},
    },
};
use artifact::{
    board::Board,
    project::{Project as _, fs},
};
use bezel::{
    gpui::{App, ClipboardItem, Context, EntityId, EventEmitter, Window},
    theme::{self, Brand, Tint, Vibrancy, appearance::AppearanceMode},
    ui::{icons::Icon, input},
};
use cacp::schema::SessionConfigOptionValue;
use editor::Mode;
use futures::{StreamExt as _, channel::mpsc};
use mcp::rail::{self, Change};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

// The rest of `impl Workspace`, split by what each part touches rather than
// left as one 1500-line block. Every one of them continues this module's
// scope — see the note at the head of each.
mod articles;
mod boards;
mod layouts;
pub use layouts::Showing;
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

pub struct Workspace {
    pub settings: Settings,
    pub projects: Vec<Project>,
    pub active: Option<usize>,
    pub appearance: AppearanceMode,
    /// Whether the window is held opaque, or nothing for the appearance's
    /// own answer — see [`vibrancy`].
    pub opaque: Option<bool>,
    pub cursor_blink: bool,
    /// Session ids are minted here and never reused, so a card's link to the
    /// session it opened stays unambiguous for the life of the process.
    next_id: u64,
    /// The body size the type ladder is scaled against, in points.
    pub text_size: f32,
    pub article_font_size: Option<f32>,
    pub terminal_font_size: f32,
    pub file_font_size: f32,
    /// The hue the greys carry, and how much of it.
    pub tint: Tint,
    /// How wide a page that has not been set either way is drawn — see
    /// [`crate::model::state::State::wide_pages`].
    pub wide_pages: bool,
    pub indent_project_rows: bool,
    /// Whether a long line in a code block wraps rather than scrolling — see
    /// [`apply_wrap_code`].
    pub wrap_code: bool,
    /// Whether the window is showing the frame meter. Runtime only — a switch
    /// you left on is not a preference worth restoring.
    pub meter: bool,
    /// The registry's mark for each configured agent, by name. Empty until the
    /// catalog lands, and stays empty offline.
    agent_icons: HashMap<String, Icon>,
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
    /// The arrangements this machine holds, and which one the window is
    /// showing. The window's rather than a project's: a layout can hold panes
    /// from several — see [`layouts`].
    pub layouts: Vec<artifact::layout::Layout>,
    pub layout: Option<usize>,
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
        let look = settings.appearance;
        bezel::ui::scroll::set_visibility(look.scrollbars.into(), cx);
        editor::set_text_size(
            cx,
            editor::TextSize {
                step: 1.,
                min: settings::CONTENT_TEXT_SIZE.0,
                max: settings::CONTENT_TEXT_SIZE.1,
            },
        );
        crate::model::typography::set_terminal_size(look.terminal_font_size, cx);
        crate::model::typography::set_file_size(look.file_font_size, cx);
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: look.mode,
            opaque: look.opaque,
            cursor_blink: look.cursor_blink,
            text_size: look.text_size,
            article_font_size: look.article_font_size,
            terminal_font_size: look.terminal_font_size,
            file_font_size: look.file_font_size,
            tint: Tint::new(look.hue, look.chroma),
            wide_pages: look.wide_pages,
            indent_project_rows: look.indent_project_rows,
            wrap_code: look.wrap_code,
            meter: false,
            next_id: 0,
            agent_icons: HashMap::new(),
            last: state.last,
            order: state.order,
            pinned: state.pinned,
            layouts: crate::model::layouts::all(),
            layout: None,
        };
        for ix in restore {
            this.restore_sessions(ix);
            this.watch_project(ix, cx);
        }
        this.open_last_entry(cx);
        this.load_agent_icons(cx);
        this.refresh_door();
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
        });
    }

    /// The other half of [`Self::save`]: the preferences, into the file a
    /// person edits. Split because the two move on different clocks — opening
    /// a project rewrites the bookkeeping and must not touch `settings.toml`,
    /// where somebody's comments live.
    fn save_appearance(&self) {
        let _ = settings::set_appearance(&settings::Appearance {
            mode: self.appearance,
            opaque: self.opaque,
            cursor_blink: self.cursor_blink,
            text_size: self.text_size,
            article_font_size: self.article_font_size,
            terminal_font_size: self.terminal_font_size,
            file_font_size: self.file_font_size,
            hue: self.tint.hue,
            chroma: self.tint.chroma,
            wide_pages: self.wide_pages,
            indent_project_rows: self.indent_project_rows,
            scrollbars: self.settings.appearance.scrollbars,
            sidebar_scrollbars: self.settings.appearance.sidebar_scrollbars,
            wrap_code: self.wrap_code,
        });
    }

    // ── agents ───────────────────────────────────────────────────────

    /// Fetch the catalog and keep each configured agent's icon. Off the UI
    /// thread — the registry is a blocking fetch on a cold cache — and a
    /// failure just leaves the map empty.
    fn load_agent_icons(&mut self, cx: &mut Context<Self>) {
        let configured = self.settings.agents.clone();
        cx.spawn(async move |this, cx| {
            let icons = cx
                .background_executor()
                .spawn(async move { agent::icons(&configured) })
                .await;
            let _ = this.update(cx, |workspace, cx| {
                workspace.agent_icons = icons;
                cx.notify();
            });
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
            self.readopt_agents();
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
        bezel::theme::appearance::set_mode(mode, cx);
        self.save_appearance();
        cx.notify();
    }

    /// The same window's other choice — see [`vibrancy`] for what each state
    /// asks of the theme.
    pub fn set_opaque(&mut self, opaque: bool, cx: &mut Context<Self>) {
        self.opaque = Some(opaque);
        apply_transparency(self.opaque, cx);
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
        agent::serve::serve(self.settings.mcp.serve && self.settings.features.sessions);
        agent::serve::set_write(self.settings.mcp.write);
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

    /// Where the tools answer, while they do.
    pub fn mcp_url(&self) -> Option<String> {
        agent::serve::url()
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

    pub fn set_auto_update(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_auto_update(on).is_err() {
            return;
        }
        self.settings.auto_update = on;
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
        input::set_caret_blink(blink, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_text_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.text_size = points;
        theme::set_base_text_size(points, cx);
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

    pub fn set_file_font_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.file_font_size = settings::clamp_content_text_size(points);
        crate::model::typography::set_file_size(self.file_font_size, cx);
        self.save_appearance();
        cx.notify();
    }

    pub fn set_terminal_font_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.terminal_font_size = settings::clamp_content_text_size(points);
        crate::model::typography::set_terminal_size(self.terminal_font_size, cx);
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
        bezel::ui::scroll::set_visibility(look.scrollbars.into(), cx);
        self.save_appearance();
        cx.refresh_windows();
        cx.notify();
    }

    pub fn set_indent_project_rows(&mut self, indent: bool, cx: &mut Context<Self>) {
        self.indent_project_rows = indent;
        self.save_appearance();
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
    theme::set_brand(
        Brand {
            tint,
            ..theme::brand(cx)
        },
        cx,
    );
}

/// What the switch asks of the brand.
///
/// Never [`Vibrancy::On`]: bezel's light palette carries no frosted tokens.
/// [`Vibrancy::Auto`] is frost in dark and opaque in light; [`Vibrancy::Off`]
/// is opaque in both.
pub fn vibrancy(opaque: Option<bool>) -> Vibrancy {
    match opaque {
        Some(true) => Vibrancy::Off,
        None | Some(false) => Vibrancy::Auto,
    }
}

/// How a fence breaks its lines, handed to the renderer that paints one.
///
/// Every document at once, the article's and the transcript's alike: one
/// answer is installed for the app, the way the highlighter and the type
/// ladder are. There is nowhere narrower to put it — a fence is painted by
/// `markdown::render`, which takes no per-surface layout.
pub fn apply_wrap_code(wrap: bool, cx: &mut App) {
    markdown::set_layout(cx, markdown::Layout { wrap_code: wrap });
}

/// Hand the answer to bezel, which reapplies it on every light/dark switch
/// from then on — including the one the OS makes at sunset, which reaches
/// nothing of ours.
pub fn apply_transparency(opaque: Option<bool>, cx: &mut App) {
    theme::set_brand(
        Brand {
            vibrancy: vibrancy(opaque),
            ..theme::brand(cx)
        },
        cx,
    );
}
