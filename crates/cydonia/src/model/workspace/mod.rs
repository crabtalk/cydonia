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
    theme::{self, Brand, Theme, Tint, appearance::AppearanceMode},
    ui::{icons::Icon, input},
};
use cacp::schema::SessionConfigOptionValue;
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
    pub reduce_transparency: bool,
    pub cursor_blink: bool,
    /// Session ids are minted here and never reused, so a card's link to the
    /// session it opened stays unambiguous for the life of the process.
    next_id: u64,
    /// The body size the type ladder is scaled against, in points.
    pub text_size: f32,
    /// The hue the greys carry, and how much of it.
    pub tint: Tint,
    /// Whether the window is showing the frame meter. Runtime only — a switch
    /// you left on is not a preference worth restoring.
    pub meter: bool,
    /// The registry's mark for each configured agent, by name. Empty until the
    /// catalog lands, and stays empty offline.
    agent_icons: HashMap<String, Icon>,
    /// What each project was last showing, by project path — where a launch
    /// puts you back.
    last: BTreeMap<PathBuf, state::Entry>,
}

impl Workspace {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
        let projects: Vec<Project> = state.projects.into_iter().map(Project::new).collect();
        let active = (!projects.is_empty()).then_some(state.active);
        let restore: Vec<usize> = (0..projects.len()).collect();
        let mut this = Self {
            settings,
            projects,
            active,
            appearance: state.appearance,
            reduce_transparency: state.reduce_transparency,
            cursor_blink: state.cursor_blink,
            text_size: state.text_size,
            tint: Tint::new(state.hue, state.chroma),
            meter: false,
            next_id: 0,
            agent_icons: HashMap::new(),
            last: state.last,
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
            appearance: self.appearance,
            reduce_transparency: self.reduce_transparency,
            cursor_blink: self.cursor_blink,
            text_size: self.text_size,
            hue: self.tint.hue,
            chroma: self.tint.chroma,
            last: self.last.clone(),
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
    pub fn preferred_agent(&self) -> Option<settings::Agent> {
        self.active_session()
            .map(|chat| chat.entry.clone())
            .or_else(|| self.settings.agents.first().cloned())
    }

    /// Re-read `settings.toml`. Installing an agent writes that file, and
    /// reading it back is what keeps this list identical to it — cheaper than
    /// a second copy of the rule for which entry an install replaces.
    pub fn reload_settings(&mut self, cx: &mut Context<Self>) {
        if let Ok(settings) = settings::load() {
            self.settings = settings;
        }
        cx.notify();
    }

    /// The settings window's choice. bezel repaints on `set_mode`; the state
    /// file is what makes it survive a relaunch.
    pub fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.appearance = mode;
        bezel::theme::appearance::set_mode(mode, cx);
        self.save();
        cx.notify();
    }

    /// The same window's other choice.
    pub fn set_reduce_transparency(&mut self, reduce: bool, cx: &mut Context<Self>) {
        self.reduce_transparency = reduce;
        apply_transparency(reduce, cx);
        self.save();
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
        self.save();
        cx.notify();
    }

    pub fn set_text_size(&mut self, points: f32, cx: &mut Context<Self>) {
        self.text_size = points;
        theme::set_base_text_size(points, cx);
        self.save();
        cx.notify();
    }

    pub fn set_tint(&mut self, tint: Tint, cx: &mut Context<Self>) {
        self.tint = tint;
        apply_tint(tint, cx);
        self.save();
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

/// Two answers, because bezel asks two questions: the window stops compositing
/// translucent, and the tint over it goes opaque. Chrome keeps its layers —
/// an opaque window carrying them is what this setting asks for, and what the
/// system's own does not do.
pub fn apply_transparency(reduce: bool, cx: &mut App) {
    let alpha = if reduce { 1.0 } else { Theme::VIBRANCY_ALPHA };
    theme::set_brand(
        Brand {
            vibrancy_alpha: alpha,
            vibrancy: !reduce,
            ..theme::brand(cx)
        },
        cx,
    );
}
