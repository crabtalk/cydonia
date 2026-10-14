//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

use crate::view::kit::menu::Menu;
use crate::view::kit::rename::{NameEvent, NameField};
use crate::view::window::chrome;
use crate::view::window::lights::{self, Lights};
#[cfg(feature = "desktop")]
use crate::view::{
    component::terminal::TerminalPanel,
    settings::{self, SettingsWindow},
};
use crate::{
    model::{
        session::ChatSession,
        settings::Settings,
        state::State,
        workspace::{Reloaded, Showing, Workspace},
    },
    view::{
        board,
        board::info,
        component::{
            composer::{Composer, ComposerEvent},
            meter,
        },
        kit::watch::Watch,
        pane::{Pane, Tab},
        sidebar::{Dragged, Renaming, Row, Sidebar},
        window::menubar::CloseWindow,
        window::modal::ModalLayer,
        window::section::Section,
    },
};
use anyhow::Result;
use artifact::kind::Kind;
use artifact::space::Member;
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Bounds, Context, Div, DragMoveEvent, Empty, Entity,
        FocusHandle, Focusable, Hsla, KeyBinding, PathPromptOptions, Pixels, Point, Render,
        TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, actions, div, point,
        prelude::*, px, size,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset, appearance},
    ui::{
        context_menu::ContextMenu,
        docking, drag,
        floating::Floating,
        icons,
        stats::Stats,
        widgets::{ButtonStyle, Buttons, Content, SplitDrag},
    },
};
use std::rc::Rc;

pub(crate) const SIDEBAR_WIDTH: f32 = 200.;

const SIDEBAR_WIDTH_MIN: f32 = 180.;

const SIDEBAR_WIDTH_MAX: f32 = 420.;

/// The sidebar's gutter: a row's outer margin, and the padding inside it.
pub(crate) const SIDEBAR_GUTTER: f32 = 8.;

/// The header strip's height, which the traffic lights centre in: macOS 26's
/// 14px lights with [`TRAFFIC_LIGHT_INSET`] above and below them.
pub(crate) const HEADER_HEIGHT: f32 = 32.;

/// The room the pill keeps above and below its content.
pub(crate) const COMPOSER_INSET: f32 = 4.;

/// How far the floating composer stands off the column's bottom edge.
pub(crate) const COMPOSER_BOTTOM: f32 = 20.;

/// How wide the composer's column runs before it stops growing, and the air it
/// keeps either side of itself inside that.
pub(crate) const COMPOSER_COLUMN: f32 = 720.;

pub(crate) const COMPOSER_MARGIN: f32 = 24.;

/// How far the close button stands in from the window's left edge: AppKit's
/// own place for it in a transparent, full-size-content titlebar on macOS 26.
pub(crate) const TRAFFIC_LIGHT_INSET: f32 = 9.;

/// The gap the header keeps at the window's edges.
pub(crate) const HEADER_INSET: f32 = 16.;

/// How far the glyph of a control at the end of a row stands from its
/// column's edge: the lights' own inset, mirrored. Measured to the glyph, not
/// the button around it.
pub(crate) const EDGE: f32 = TRAFFIC_LIGHT_INSET;

/// What an icon button leaves on each side of its glyph.
const BUTTON_SLACK: f32 = (Theme::BUTTON_HEIGHT - ICON_GLYPH) / 2.;

/// The glyph size of `Buttons::icon_button`, which bezel does not export.
const ICON_GLYPH: f32 = 14.;

/// The trailing padding that puts an icon button's glyph at [`EDGE`].
pub(crate) const BUTTON_EDGE: f32 = EDGE - BUTTON_SLACK;

/// The gap between the green light and the glyph of the first control past
/// it.
const LIGHTS_GAP: f32 = 16.;

actions!(
    cydonia,
    [
        NewSession,
        NewSessionNext,
        NewBoard,
        NewArticle,
        NewTable,
        OpenProject,
        CloseProject,
        OpenSettings,
        ToggleSidebar,
        ToggleTerminal,
        ToggleChanges,
        OpenFiles,
        OpenReview,
        NextEntry,
        PrevEntry,
        NextPane,
        PrevPane,
        ClosePane,
        ZoomPane,
        CopySelection,
        GoBack,
        GoForward
    ]
);

/// File › New Session With: a session on the agent it names. By name, because
/// that is what the menu was built from — an index would open the wrong agent
/// the moment one was installed ahead of it and the bar not yet rebuilt.
///
/// No JSON: the menu is the only thing that dispatches it, and nobody writes
/// an agent's name into a keymap.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = cydonia, no_json)]
pub struct NewSessionWith {
    pub agent: String,
}

/// Open the space at `index` in the sidebar's order — `cmd-1` through
/// `cmd-8` — or the last one for `None`, `cmd-9`. An index past the last space
/// does nothing.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = cydonia, no_json)]
pub struct SwitchSpace {
    pub index: Option<usize>,
}

/// The pill at rest, and the agent mark beside it. Half of it is the stadium's
/// radius.
pub(crate) fn composer_height() -> f32 {
    composer_disc() + 2. * COMPOSER_INSET
}

/// Keep the send target comfortable at small text sizes, and grow with the
/// line box when the text-size setting needs more room.
pub(crate) fn composer_disc() -> f32 {
    TextStyle::Body.painted_line_height().max(24.)
}

/// What the composer itself is at its widest — the column, less its own air.
///
/// The `/` picker is capped at this. Its rows carry a sentence each and the
/// card sizes to the longest of them, so on a wide window it opened as far as
/// the window allowed: a menu reaching past the box it came out of stops
/// reading as that box's menu.
pub(crate) fn composer_width() -> f32 {
    COMPOSER_COLUMN - 2. * COMPOSER_MARGIN
}

/// The sidebar's fill: bezel's frost tint, or opaque the chrome tone — the
/// light palette's `surface` is the grey the content plane's white sits inside.
pub(crate) fn sidebar_bg(theme: &Theme) -> Hsla {
    match theme.vibrancy {
        true => theme.vibrancy_tint(),
        false => theme.surface,
    }
}

/// The lights' origin, for `TitlebarOptions::traffic_light_position`, until
/// [`lights::fit`] has measured them.
pub(crate) fn traffic_lights() -> Point<Pixels> {
    point(px(TRAFFIC_LIGHT_INSET), px(TRAFFIC_LIGHT_INSET))
}

/// The band across the top of a column, and the only place its height and its
/// inset are written: the header, the sidebar's, a pane's in a space, and the
/// right panel's are all this row. A control in one stands where the same
/// control stands in the next.
///
/// The leading inset is the caller's only where the traffic lights take it —
/// see [`toolbar_inset`].
pub(crate) fn band() -> Div {
    div()
        .flex_none()
        .h(px(HEADER_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .pl(px(HEADER_INSET))
        .pr(px(BUTTON_EDGE))
}

/// Where the toolbar's own controls start at the window's left edge: the
/// button whose glyph stands [`LIGHTS_GAP`] past the green light, or in full
/// screen, where there are no lights, in line with the sidebar rows' icons.
pub(crate) fn toolbar_inset(window: &Window, cx: &App) -> f32 {
    match (window.is_fullscreen(), cfg!(target_os = "macos")) {
        (true, _) => 2. * SIDEBAR_GUTTER - BUTTON_SLACK,
        (false, true) => Lights::of(cx).end() + LIGHTS_GAP - BUTTON_SLACK,
        (false, false) => HEADER_INSET,
    }
}

/// The chords the window keeps whatever the reader says — the commands it also
/// answers to are bound from [`crate::view::keymap`], which is where they can
/// be moved.
pub fn bindings() -> Vec<KeyBinding> {
    let spaces = (1..=9).map(|n| {
        KeyBinding::new(
            &format!("secondary-{n}"),
            SwitchSpace {
                index: (n < 9).then(|| n - 1),
            },
            Some("Cydonia"),
        )
    });
    spaces
        .chain([
            // What a browser binds its tabs to, and the only chord these answer to:
            // the menu bar cannot draw it — gpui has no macOS equivalent for `tab`,
            // so AppKit is handed the word where the API takes one character and
            // shows ⌃T — and a chord it *can* draw is claimed by AppKit before the
            // window is ever offered it.
            //
            // Scoped to the root rather than left contextless, so that a surface
            // with a row of its own can take the chord for its own row: a binding
            // with no predicate ranks at the depth of the whole stack, which puts
            // it *above* every scoped one rather than below — see
            // `Keymap::binding_enabled`. The `cmd-c` fallback below is the same
            // trick for the same reason.
            KeyBinding::new("ctrl-tab", NextEntry, Some("Cydonia")),
            KeyBinding::new("ctrl-shift-tab", PrevEntry, Some("Cydonia")),
            // The panes of a space, on the chords beside the ones that step
            // through entries.
            KeyBinding::new("ctrl-alt-tab", NextPane, Some("Cydonia")),
            KeyBinding::new("ctrl-alt-shift-tab", PrevPane, Some("Cydonia")),
            KeyBinding::new("ctrl-alt-w", ClosePane, Some("Cydonia")),
            KeyBinding::new("ctrl-alt-z", ZoomPane, Some("Cydonia")),
            // Scope the fallback to the root so focused text surfaces take priority.
            KeyBinding::new(
                "secondary-c",
                CopySelection,
                Some(crate::view::keymap::platform(
                    "Cydonia",
                    "Cydonia && !CydoniaTerminal",
                )),
            ),
        ])
        .collect()
}

impl Cydonia {
    fn save_window(&mut self, cx: &mut App) {
        if let Some(frame) = self.window_frame.take() {
            self.workspace
                .update(cx, |workspace, _| workspace.set_window(frame));
        }
    }
}

/// The frame a window was left at, where some display still shows it.
fn restored(frame: crate::model::state::Frame, cx: &App) -> Option<WindowBounds> {
    use crate::model::state::Mode;
    let valid = [frame.x, frame.y, frame.width, frame.height]
        .iter()
        .all(|n| n.is_finite())
        && frame.width >= 600.
        && frame.height >= 320.;
    let bounds = Bounds::new(
        point(px(frame.x), px(frame.y)),
        size(px(frame.width), px(frame.height)),
    );
    let shown = cx
        .displays()
        .iter()
        .any(|display| display.bounds().intersects(&bounds));
    (valid && shown).then_some(match frame.mode {
        Mode::Windowed => WindowBounds::Windowed(bounds),
        Mode::Maximized => WindowBounds::Maximized(bounds),
        Mode::Fullscreen => WindowBounds::Fullscreen(bounds),
    })
}

fn frame_of(bounds: WindowBounds) -> crate::model::state::Frame {
    use crate::model::state::Mode;
    let (bounds, mode) = match bounds {
        WindowBounds::Windowed(bounds) => (bounds, Mode::Windowed),
        WindowBounds::Maximized(bounds) => (bounds, Mode::Maximized),
        WindowBounds::Fullscreen(bounds) => (bounds, Mode::Fullscreen),
    };
    crate::model::state::Frame {
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
        mode,
    }
}

/// Open the workspace window. Called at launch, and again when the Dock
/// reopens an app whose window ⌘W closed.
pub fn open(settings: Settings, state: State, cx: &mut App) -> Result<WindowHandle<Cydonia>> {
    let bounds = state
        .window
        .and_then(|frame| restored(frame, cx))
        .unwrap_or_else(|| {
            WindowBounds::Windowed(Bounds::centered(None, size(px(1100.), px(760.)), cx))
        });
    cx.open_window(
        WindowOptions {
            window_bounds: Some(bounds),
            // No strip of its own: the traffic lights sit in the nav, so the
            // window owes no titlebar above it.
            titlebar: Some(TitlebarOptions {
                title: Some("Cydonia".into()),
                appears_transparent: true,
                traffic_light_position: Some(traffic_lights()),
            }),
            // Glass needs a blurred window background to blur into. A window
            // that frames itself opens transparent, or its frame band is
            // filled; bezel's reapply keeps it so on each appearance switch.
            window_background: match crate::view::window::chrome::decorations() {
                Some(_) => bezel::gpui::WindowBackgroundAppearance::Transparent,
                None => Theme::of(cx).window_background_appearance(),
            },
            window_background_blur: Theme::of(cx).window_blur.into(),
            window_min_size: Some(size(px(600.), px(320.))),
            app_id: Some("cydonia".into()),
            window_decorations: crate::view::window::chrome::decorations(),
            app_owns_titlebar_drag: true,
            ..Default::default()
        },
        |window, cx| {
            appearance::observe_window(window, cx).detach();
            lights::fit(window, cx);
            cx.new(|cx| {
                let mut root = Cydonia::new(settings, state, window, cx);
                root.restore_panel_layout();
                cx.observe_window_bounds(window, |root: &mut Cydonia, window, cx| {
                    root.window_frame = Some(frame_of(window.window_bounds()));
                    root.window_save = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(400))
                            .await;
                        this.update(cx, |this, cx| {
                            this.window_save = None;
                            this.save_window(cx);
                        })
                        .ok();
                    }));
                })
                .detach();
                cx.on_release(|root: &mut Cydonia, cx| {
                    root.save_panel_layout(cx);
                    root.save_window(cx);
                })
                .detach();
                // ⌘Q tears the process down without releasing the root, so a
                // release hook alone loses everything dragged in the session
                // that quit.
                cx.on_app_quit(|root: &mut Cydonia, cx| {
                    root.save_panel_layout(cx);
                    root.save_window(cx);
                    async {}
                })
                .detach();
                root
            })
        },
    )
}

/// What a press on a turn's notification — see [`crate::model::notify`] —
/// does: bring the app back, and open the session that finished.
///
/// Registered once, at startup. The window it lands on is whichever one is
/// showing the project that session is in; a notification whose session has
/// since gone opens nothing but the app.
pub fn on_notification(cx: &mut App) {
    cx.on_system_notification_response(|response, cx| {
        let Some(id) = crate::model::notify::session_of(&response.tag) else {
            return;
        };
        cx.activate(true);
        for window in cx.windows() {
            let Some(window) = window.downcast::<Cydonia>() else {
                continue;
            };
            let _ = window.update(cx, |root, window, cx| root.select_session(id, window, cx));
        }
    });
}

/// One step from `at` through `len` entries, wrapping — a list of none has
/// nowhere to land.
fn stepped(at: Option<usize>, len: usize, step: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let at = at.unwrap_or(0) as isize;
    Some((at + step).rem_euclid(len as isize) as usize)
}

/// The root view. It owns no app state — only the window's own chrome: how
/// wide the sidebar is, which menu is open, and what a dialog is asking about.
///
/// What is being shown, and everything the showing of it needs, is the
/// [`Tab`]'s.
pub struct Cydonia {
    pub(crate) workspace: Entity<Workspace>,
    /// The panes on screen, and which of them has the focus.
    ///
    /// One unless a space is open — see [`Tab`]. The order is the order the
    /// arrangement lays them out, so stepping through them is stepping across
    /// the window.
    pub(crate) leaves: Vec<Entity<Tab>>,
    /// The panes of the open space, each holding its tabs — see [`Pane`].
    pub(crate) panes: Vec<Entity<Pane>>,
    /// Every session, observed — see [`Self::watch_sessions`].
    session_watches: Watch,
    /// The projects sidebar, drawn under the band at the top of its column.
    pub(crate) sidebar: Entity<Sidebar>,
    pub(crate) focused: usize,
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_width: f32,
    /// The window's bottom panel: its shell, and whether it is up.
    ///
    /// One to a window, like the sidebar and the right panel — every pane and
    /// every space shows this same one, and hiding it keeps its processes
    /// alive. It opens in the directory of whatever was in front at the time
    /// and stays there; `cmd-t` is how a tab somewhere else is had.
    #[cfg(feature = "desktop")]
    pub(crate) terminal: Option<(bool, Entity<TerminalPanel>)>,
    /// Whether the right panel is up for the directory in front. Which
    /// directory that is lives in [`Cydonia::changes_for`]: the answer is per
    /// directory, and this field is the one it is currently speaking for.
    pub(crate) changes_open: bool,
    /// The directory [`Cydonia::changes_open`] answers for, and what every
    /// other directory was left at. Read back through
    /// [`Cydonia::sync_changes`] on the way into one, and seeded from what was
    /// written down for a directory that has not been looked at yet this run.
    ///
    /// A directory rather than an entry: every tab the panel holds is about
    /// one — a diff of it, a tree of it, a shell in it — so a session, an
    /// article and a board that share a working directory share the panel.
    /// [`Cydonia::shell_cwd`] is what names it, the same answer the bottom
    /// panel's shells open in.
    pub(crate) changes_for: Option<std::path::PathBuf>,
    pub(crate) changes_shown: std::collections::HashMap<std::path::PathBuf, bool>,
    /// The share of the row the right-hand panel was dragged to, and `None`
    /// for one nobody has dragged. See [`crate::view::detail::panel_width`].
    pub(crate) changes_share: Option<f32>,
    /// The pending write of a width being dragged — dropped and replaced by
    /// each move, so only a drag that stopped reaches the disk. See
    /// [`Cydonia::save_panel_layout_settled`].
    pub(crate) panel_save: Option<bezel::gpui::Task<()>>,
    /// The pending write of the window's frame, replaced by each move or
    /// resize so only one that stopped reaches the disk.
    window_save: Option<bezel::gpui::Task<()>>,
    /// The frame that write carries, taken by whichever of it and the quit
    /// gets there first.
    window_frame: Option<crate::model::state::Frame>,
    /// How tall the bottom panel was dragged, and `None` for one nobody has
    /// dragged. See [`crate::view::detail::panel_height`].
    pub(crate) terminal_height: Option<f32>,
    pub(crate) changes: Option<Entity<crate::view::component::panel::Panel>>,
    /// The press on a [`crate::view::window::chrome::grip`], shared by every band in the
    /// window that carries one.
    pub(crate) drag: bezel::ui::titlebar::DragState,
    pub(crate) right_panels:
        std::collections::HashMap<std::path::PathBuf, Entity<crate::view::component::panel::Panel>>,
    /// The right panel while the library is up — see [`Cydonia::library_panel`].
    pub(crate) library_panel: Option<Entity<crate::view::component::panel::Panel>>,
    /// The buffer each card's orb paints into, by card id — see
    /// [`board::Marks`].
    pub(crate) card_marks: board::Marks,
    /// What each card's text parses to, by its text — see [`board::Docs`].
    /// Shared with every board view.
    pub(crate) card_docs: Rc<board::Docs>,
    /// The entries the window has been on — see [`crate::view::window::history`].
    pub(crate) history: crate::view::window::history::History,
    /// The composer each session drawn in a drawer types into, by the
    /// session's id — see [`Cydonia::session_transcript`].
    pub(crate) session_cards:
        std::collections::HashMap<u64, Entity<crate::view::component::composer::Composer>>,
    /// The thread each article's composer opens, by the article's file — see
    /// [`crate::view::article::thread`].
    pub(crate) threads: std::collections::HashMap<
        std::path::PathBuf,
        Entity<crate::view::article::thread::ThreadView>,
    >,
    /// The pane the last press came down in: `None` for the one a window with
    /// no space open shows. Where a link opens — see
    /// [`Cydonia::open_reference`].
    pub(crate) pressed_pane: Option<Member>,
    #[cfg(feature = "desktop")]
    settings_window: Option<WindowHandle<SettingsWindow>>,
    /// The modal over the window, while one is up.
    pub(crate) modals: Entity<ModalLayer>,
    /// Where a sidebar row or a pane's tab lands on a pane. The panes are its
    /// targets, by the member in front of each; the window is its surface.
    pub(crate) dock: docking::Dock<Member, Dragged>,
    /// Every board on screen: cards between lanes and boards, lanes and list
    /// groups along their own board.
    pub(crate) board_sort: drag::Domain<board::BoardRegion, board::BoardItem>,
    /// The tabs that have been brought to the front, most recent last, across
    /// every pane. Read when a tab closes, to land on the one that was in
    /// front before it rather than on a neighbour in the strip — see
    /// [`Cydonia::close_pane`]. Runtime only, for the same reason `fronts` is.
    pub(crate) tab_history: Vec<Member>,
    /// The library, while it stands over the panes — see
    /// [`crate::view::library`].
    pub(crate) library: Option<Entity<crate::view::library::Library>>,
    /// The board identity panel, while it is open — see [`header::BoardInfo`].
    pub(crate) info: Option<info::BoardInfo>,
    /// Whether the press now being handled landed on the name of the board
    /// whose panel is open — read by [`Cydonia::toggle_info`] and nothing else.
    pub(crate) info_pressed: bool,
    /// The menu that is up, of every menu the window's screens open — see
    /// [`crate::view::kit::menu`].
    pub(crate) menu: ContextMenu<Cydonia, Menu>,
    /// What the band's name field is attached to, and the field itself.
    pub(crate) renaming: Option<Renaming>,
    pub(crate) name_field: Entity<NameField>,
    /// The sidebar's search across projects — see [`crate::view::search`].
    pub(crate) search: Entity<crate::view::search::Search>,
    meter: Entity<Stats>,
    meter_at: Floating,
    /// Where the focus rests when no field holds it — a board, a table and a
    /// transcript have none — so the bindings below always have a path here.
    pub(crate) focus: FocusHandle,
}

impl Cydonia {
    /// The pane with the focus — what a command without a pane of its own acts
    /// on, and what the sidebar lights.
    ///
    /// Never empty: a window always draws at least one pane, and closing the
    /// last one is closing the space, not the pane.
    pub(crate) fn leaf(&self) -> &Entity<Tab> {
        let at = self.focused.min(self.leaves.len().saturating_sub(1));
        &self.leaves[at]
    }

    /// What the model saying something changed asks of the window: the applied
    /// search read again, and the composers pointed at what they are on.
    fn model_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.watch_sessions(window, cx);
        self.search.update(cx, crate::view::search::Search::refresh);
        let previous = self.leaf().read(cx).composer.read(cx).session();
        self.sync_composer(cx);
        let current = self.leaf().read(cx).composer.read(cx).session();
        // An article's composer takes the focus only from a press into it.
        if current.is_some()
            && current != previous
            && self.leaf().read(cx).thread_article(cx).is_none()
        {
            window.focus(&self.composer_focus_handle(cx), cx);
        }
    }

    /// Observe every session the workspace holds: the composers follow each
    /// one's own change.
    fn watch_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chats: Vec<Entity<ChatSession>> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|project| &project.sessions)
            .map(|held| held.chat.clone())
            .collect();
        let mut watches = std::mem::take(&mut self.session_watches);
        watches.set_in(&chats, window, cx, |this, _, window, cx| {
            this.model_changed(window, cx)
        });
        self.session_watches = watches;
    }

    /// The sidebar's column: the band the window draws at its top — the
    /// lights, the fold with back and forward, the grip — over the sidebar.
    fn sidebar_column(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(sidebar_bg(&theme))
            .flex()
            .flex_col()
            // The fold just past the lights, where the pane header puts it
            // with the sidebar folded: it keeps its place across the toggle.
            .child(
                band()
                    .pl(px(toolbar_inset(window, cx)))
                    .when(
                        chrome::has(bezel::ui::titlebar::CaptionSide::Left, window, cx),
                        |band| band.pl_0(),
                    )
                    .children(chrome::caption(
                        bezel::ui::titlebar::CaptionSide::Left,
                        window,
                        cx,
                    ))
                    .child(self.fold_controls(cx))
                    .child(chrome::grip("sidebar-grip", &self.drag, window)),
            )
            .child(
                self.sidebar.clone().cached(
                    gpui::StyleRefinement::default()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .flex()
                        .flex_col(),
                ),
            )
    }

    /// Hand the sidebar what the window is showing, where that moved.
    fn sync_sidebar(&mut self, cx: &mut Context<Self>) {
        let lit = self.lit(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_lit(lit, cx));
    }

    /// Tell each pane whether it is the one with the focus, where that moved.
    pub(crate) fn sync_focus(&mut self, cx: &mut Context<Self>) {
        let focused = self.leaf().entity_id();
        for pane in self.leaves.clone() {
            let on = pane.entity_id() == focused;
            if pane.read(cx).focused != on {
                pane.update(cx, |pane, cx| {
                    pane.focused = on;
                    cx.notify();
                });
            }
        }
    }

    /// A pane on `entry`, its composer wired to this window.
    fn new_pane(
        &self,
        entry: Option<Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Tab> {
        let composer = cx.new(Composer::new);
        let (workspace, root) = (self.workspace.clone(), cx.entity().downgrade());
        let (docs, sort) = (self.card_docs.clone(), self.board_sort.clone());
        let applied = self.search.read(cx).query().cloned();
        let pane = cx.new(|cx| {
            let mut pane = Tab::new(entry, workspace, root, docs, sort, composer.clone(), cx);
            pane.applied = applied;
            pane
        });
        Self::wire_composer(&pane, &composer, window, cx);
        pane
    }

    /// Send what `composer` reports to where `pane` points it now: the article
    /// the pane shows, or the session the composer was last pointed at by
    /// [`Cydonia::sync_composer`]. A pane's entry and what it shows change
    /// over its life, so both are read at the event.
    fn wire_composer(
        pane: &Entity<Tab>,
        composer: &Entity<Composer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = pane.downgrade();
        cx.subscribe_in(
            composer,
            window,
            move |this, composer, event: &ComposerEvent, window, cx| {
                let Some(pane) = pane.upgrade() else {
                    return;
                };
                let on = pane.read(cx).entry.clone();
                // Every one of these but a draft is somebody pressing in this
                // pane's composer, and the focus goes there first so that what
                // was typed is sent to the session it was typed under.
                //
                // A draft is the field's content changing, which
                // [`Cydonia::sync_composer`] does to every pane's composer as
                // a session is read — panes nobody pressed in included. The
                // focus would follow the last of those, which is not where
                // anybody is looking. Somebody typing has the focus already:
                // the press that put the caret in the field went through the
                // pane, which is what moves it.
                if let Some(on) = on.clone()
                    && !matches!(event, ComposerEvent::Draft(..))
                {
                    this.focus_pane(&on, window, cx);
                }
                if let Some(path) = pane.read(cx).thread_article(cx) {
                    return this.thread_event(&path, event, window, cx);
                }
                let session = composer.read(cx).session();
                this.composer_event(session, event, window, cx);
            },
        )
        .detach();
    }

    /// Reconcile the panes on screen with the open space.
    ///
    /// A pane already on an entry is kept, so switching spaces does not throw
    /// away a composer with a draft in it. Panes are ordered as the
    /// arrangement lays them out — stepping through them steps across the
    /// window.
    /// Hand the tools what is on screen — see [`mcp::rail::shown`].
    fn publish_shown(&self, cx: &App) {
        // A session is the chat the tools are talking in, not a thing shown.
        let kind = |kind: Kind| (kind != Kind::Session).then_some(kind);
        let shown = if self.leaves.iter().any(|leaf| leaf.read(cx).entry.is_some()) {
            self.leaves
                .iter()
                .enumerate()
                .filter_map(|(ix, leaf)| {
                    let entry = leaf.read(cx).entry.as_ref()?;
                    Some(mcp::rail::Shown {
                        project: entry.project.clone(),
                        kind: kind(entry.kind)?,
                        id: entry.id.clone(),
                        focused: ix == self.focused,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            let workspace = self.workspace.read(cx);
            workspace
                .landed()
                .and_then(|(project, entry)| {
                    Some(mcp::rail::Shown {
                        project: project.to_path_buf(),
                        kind: kind(entry.kind)?,
                        id: entry.id.clone(),
                        focused: true,
                    })
                })
                .into_iter()
                .collect()
        };
        let lone = shown.len() == 1;
        let shown = shown
            .into_iter()
            .map(|mut entry| {
                // A lone entry beside a focused chat is still the one in front.
                entry.focused |= lone;
                entry
            })
            .collect();
        mcp::rail::set_shown(shown);
    }

    pub(crate) fn sync_leaves(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let members = self.arrangement(cx).map(|space| space.entries());
        let Some(members) = members else {
            // No space: one pane, and the leaf kept is the focused one
            // rather than the first — leaving an arrangement leaves you in
            // the pane you were standing in, composer and draft included.
            //
            // [`Self::leaf_mut`] indexes by `self.focused`, so anything
            // written through it between a space closing and this running
            // lands on a leaf that is about to be dropped.
            let front = self.focused.min(self.leaves.len().saturating_sub(1));
            if front > 0 {
                self.leaves.swap(0, front);
            }
            self.leaves.truncate(1);
            self.focused = 0;
            self.leaf().update(cx, |pane, _| pane.entry = None);
            return;
        };
        let focused = self.leaf().read(cx).entry.clone();
        let mut kept: Vec<Entity<Tab>> = Vec::with_capacity(members.len());
        for entry in &members {
            match self
                .leaves
                .iter()
                .position(|leaf| leaf.read(cx).entry.as_ref() == Some(entry))
            {
                Some(at) => kept.push(self.leaves.remove(at)),
                None => kept.push(self.new_pane(Some(entry.clone()), window, cx)),
            }
        }
        self.leaves = kept;
        self.focused = focused
            .and_then(|on| {
                self.leaves
                    .iter()
                    .position(|leaf| leaf.read(cx).entry.as_ref() == Some(&on))
            })
            .unwrap_or(0);
        self.sync_focus(cx);
        self.sync_panes(cx);
    }

    /// Arrive at an entry, wherever the sidebar lists it.
    ///
    /// One gesture, one meaning: a row goes to where that row lives. An entry
    /// is in one space at a time — see [`Workspace::arrange`] — so it is
    /// listed either under a space or under its project, never both, and that
    /// one place says what opening it means. Under a space, the arrangement is
    /// opened and the pane focused; under a project, the window shows it alone.
    /// Where the window happened to be standing does not enter into it.
    ///
    /// `true` when the entry was found in a space and the focus went to its
    /// pane, so the caller has nothing left to open. `false` for an entry no
    /// space holds, which is the plain single-pane case.
    pub(crate) fn enter_member(
        &mut self,
        member: Option<Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.library = None;
        // A space names entries by the file they have, so one with no file yet
        // — a session that has had no turn — is in none of them.
        let held = member.as_ref().and_then(|member| {
            let at = self.workspace.read(cx).space_holding(member)?;
            Some((at, member.clone()))
        });
        let Some((at, member)) = held else {
            if self.workspace.read(cx).active_space().is_some() {
                self.workspace
                    .update(cx, |workspace, _| workspace.leave_space());
                // Down to one pane before the caller writes anything: it
                // reaches for the leaf through [`Self::leaf_mut`], which
                // indexes by [`Self::focused`] — still pointing into the panes
                // the space had, and at a leaf this is about to drop.
                self.sync_leaves(window, cx);
            }
            return false;
        };
        if self.workspace.read(cx).space != Some(at) {
            self.open_space(at, window, cx);
        }
        // The pane a tab is in is keyed by the first of its strip — see
        // [`Self::show_tab`].
        let stack = self.workspace.read(cx).stack_of(&member);
        let pane = stack.first().cloned().unwrap_or_else(|| member.clone());
        self.show_tab(&pane, &member, window, cx);
        true
    }

    /// Move the focus to the pane on this entry, and put the project's
    /// selection on what that pane shows.
    ///
    /// The selection is what every command without a pane of its own reads —
    /// see [`Workspace::active_board`] and the rest. Syncing it here is what
    /// makes "the pane you are in" the thing they act on.
    pub(crate) fn focus_pane(
        &mut self,
        entry: &Member,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self
            .leaves
            .iter()
            .position(|leaf| leaf.read(cx).entry.as_ref() == Some(entry))
        else {
            // [`Cydonia::show_tab`] parks the index past the end to force the
            // selection below, and [`Self::leaf`] clamps — so a miss left here
            // reads as the pane at the end of the space, which is some entry
            // nobody pressed.
            self.focused = self.focused.min(self.leaves.len().saturating_sub(1));
            return;
        };
        // Which leaf is the focused one and where the window's focus actually
        // sits are two facts, and they come apart: a pane can already be the
        // focused leaf while the caret is still in the composer of the pane
        // left behind. So the selection below is skipped for a leaf that was
        // already focused, and the caret is settled every time regardless.
        let moved = self.focused != at;
        self.focused = at;
        self.sync_focus(cx);
        let Some((project, showing)) = self.workspace.read(cx).showing_of(entry, cx) else {
            // A member whose project is shut draws an empty pane, which takes
            // no caret — so the focus comes back to the window rather than
            // staying wherever it was.
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        };
        if moved {
            let kind = showing.kind();
            self.leaf().update(cx, |pane, _| pane.kind = kind);
            self.workspace.update(cx, |workspace, cx| {
                workspace.select_showing(project, showing.clone(), cx);
            });
        }
        self.sync_composer(cx);
        // The press already put the focus on something inside this pane — a
        // transcript's text, a field — before bubbling here. Moving it to the
        // composer would take ⌘C away from the run just selected.
        let here = self.leaf().read(cx).focus.clone();
        if here.contains_focused(window, cx) && !here.is_focused(window) {
            cx.notify();
            return;
        }
        // The caret follows the pane into whatever it can be typed into: a
        // session's composer, a document's editor, an open card or cell. A
        // board or a table with nothing open takes none, and neither does a
        // session that cannot be sent to.
        //
        // Those land on the window's own handle rather than being left alone.
        // The caret belongs to the pane in front, so a focus left behind is a
        // composer blinking in a pane nobody is looking at — and the next thing
        // typed goes to the session that pane is on.
        let caret = match showing {
            Showing::Session(id) => self
                .workspace
                .read(cx)
                .session(id, cx)
                .is_some_and(ChatSession::resumable)
                .then(|| self.composer_focus_handle(cx)),
            Showing::Article(_) => self.pane_editor(cx).map(|editor| editor.focus_handle(cx)),
            // A board or a table takes no caret of its own, but an open card
            // or cell is a field inside one: pressing into the text a second
            // time is how the caret is moved, and settling on the pane instead
            // would take the field's focus off it every time.
            Showing::Board(id) => self
                .leaf()
                .read(cx)
                .boards
                .iter()
                .map(|view| view.read(cx))
                .find(|view| view.board.id == *id)
                .filter(|view| view.editing.is_some())
                .map(|view| view.card_field.read(cx).focus_handle(cx)),
            Showing::Table(_) => self
                .table_view_of(self.leaf().read(cx).entry.clone().as_ref(), cx)
                .map(|view| view.read(cx))
                .filter(|view| view.cell.is_some())
                .map(|view| view.cell_field.read(cx).focus_handle(cx)),
            // An app's fields take the caret when they are pressed.
            Showing::App(_) => None,
        };
        // The pane's own handle, not the window's: the root's is tracked on a
        // sibling of the panes, so landing there puts the focus outside the
        // pane and the chords the pane claims stop being reached.
        window.focus(caret.as_ref().unwrap_or(&here), cx);
        cx.notify();
    }

    /// The pane showing this entry, falling back to the focused one where the
    /// window has no space open and the caller has no entry to name.
    ///
    /// Drawing reads a pane's state through this rather than through
    /// [`Self::leaf`]: every pane drawn against the focused leaf shares one
    /// scroll and one editor between them, so moving the focus
    /// moves what the other panes are showing.
    pub(crate) fn leaf_of(&self, on: Option<&Member>, cx: &App) -> &Entity<Tab> {
        on.and_then(|on| {
            self.leaves
                .iter()
                .find(|leaf| leaf.read(cx).entry.as_ref() == Some(on))
        })
        .unwrap_or_else(|| self.leaf())
    }

    pub fn new(
        settings: Settings,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_field = cx.new(NameField::new);
        cx.subscribe(&name_field, |this, _, event: &NameEvent, cx| match event {
            NameEvent::Named(name) => this.commit_name(name.clone(), cx),
            NameEvent::Dismissed => this.dismiss_name(cx),
        })
        .detach();
        let workspace = cx.new(|cx| Workspace::new(settings, state, cx));
        Workspace::install(&workspace, cx);
        // The model is the only thing that says a session appeared or a turn
        // ended; the composer's placeholder, commands and busy state are all
        // read back from it rather than pushed by whoever caused the change.
        cx.observe_in(&workspace, window, |this, _, window, cx| {
            this.model_changed(window, cx)
        })
        .detach();
        // A re-read replaced what a pane is showing — see
        // [`Workspace::reload_project`]. The card is held by id, so it comes
        // through unless what it names is gone; the cell is still addressed by
        // where it sits, so filing it now would file it into whatever slid
        // under the index, and it is dropped.
        cx.subscribe_in(&workspace, window, |this, _, _: &Reloaded, _, cx| {
            this.drop_stale_edit(cx);
            this.rest_ribbon(cx);
            this.drop_cells(cx);
            cx.notify();
        })
        .detach();

        let search = cx.new(|_| crate::view::search::Search::new(workspace.clone()));
        // The applied query narrows every pane's find.
        cx.observe(&search, |this, _, cx| {
            this.push_finds(cx);
            cx.notify();
        })
        .detach();
        let sidebar = {
            let (workspace, root, search) =
                (workspace.clone(), cx.entity().downgrade(), search.clone());
            cx.new(|cx| Sidebar::new(workspace, root, search, cx))
        };
        let mut this = Self {
            meter: cx.new(Stats::new),
            meter_at: Floating::new(Painter::of(cx)),
            workspace,
            leaves: Vec::new(),
            panes: Vec::new(),
            session_watches: Watch::default(),
            sidebar,
            focused: 0,
            sidebar_open: true,
            sidebar_width: SIDEBAR_WIDTH,
            #[cfg(feature = "desktop")]
            terminal: None,
            changes_open: false,
            changes_for: None,
            changes_shown: Default::default(),
            changes_share: None,
            panel_save: None,
            window_save: None,
            window_frame: None,
            terminal_height: None,
            changes: None,
            drag: Default::default(),
            right_panels: Default::default(),
            library_panel: None,
            card_marks: Default::default(),
            card_docs: Default::default(),
            history: Default::default(),
            session_cards: Default::default(),
            threads: Default::default(),
            pressed_pane: None,
            #[cfg(feature = "desktop")]
            settings_window: None,
            dock: docking::Dock::new(Painter::of(cx), {
                let this = cx.entity().downgrade();
                move |item, _, cx| {
                    let label = this
                        .upgrade()
                        .map(|this| this.read(cx).label_of_dragged(item, cx))
                        .unwrap_or_default();
                    crate::view::sidebar::ghost(label.into(), Theme::of(cx))
                }
            }),
            board_sort: drag::Domain::with_ghost(Painter::of(cx), {
                let this = cx.entity().downgrade();
                move |item, window, cx| board::ghost(&this, item, window, cx)
            }),
            tab_history: Vec::new(),
            modals: cx.new(|_| ModalLayer::default()),
            library: None,
            info: None,
            info_pressed: false,
            menu: ContextMenu::default(),
            renaming: None,
            name_field,
            search,
            focus: cx.focus_handle(),
        };
        // Whatever held the focus has left the tree — the composer with the
        // chat pane, an editor with its article — and an unrendered element
        // dispatches nothing, so the window takes its focus back.
        cx.on_focus_lost(window, |this, window, cx| window.focus(&this.focus, cx))
            .detach();
        // The backstop under the watch. Coming back to the window is where a
        // dropped event costs the most and the one moment we can be sure of
        // catching, so every project is re-read on the way in — see
        // [`Workspace::reload_projects`].
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.workspace
                    .update(cx, |workspace, cx| workspace.reload_projects(cx));
            }
        })
        .detach();
        let pane = this.new_pane(None, window, cx);
        this.leaves.push(pane);
        // The window comes back on the entry it was left on — the whole point
        // of [`state::Entry`], and the pane the entry is read in is half of it.
        this.land(cx);
        this.sync_composer(cx);
        #[cfg(not(target_os = "linux"))]
        this.take_browser(window, cx);
        // Where the caret starts. The composer is drawn only over a chat it can
        // send to, and focus on an element no frame draws is focus nowhere.
        let composer = this
            .workspace
            .read(cx)
            .active_session(cx)
            .is_some_and(ChatSession::resumable)
            .then(|| this.composer_focus_handle(cx));
        window.focus(composer.as_ref().unwrap_or(&this.focus), cx);
        this.watch_sessions(window, cx);
        this
    }

    /// Open a session on whichever agent the last one ran on, or on the first
    /// one configured.
    ///
    /// With none configured there is nothing to open a session *on*, and a
    /// chat pane switched to over no session is a blank one. A fresh install
    /// names no agent — see [`crate::model::settings::Settings::default`] — so
    /// this is where most people meet the feature: it sends them to the
    /// section that installs one, the same place the composer's own
    /// `Install an agent…` goes.
    pub(crate) fn new_session_action(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asked = self.workspace.read(cx).preferred_agent(cx);
        // Same as [`Self::pick_agent`]: a session with no file yet is in no
        // arrangement, so making one leaves whatever space is up.
        self.enter_member(None, window, cx);
        // Nothing to open one on: the pane says so and offers the install,
        // which is the same notice a session whose agent has gone stands
        // under. The window jumping to Settings on its own answered a question
        // it had not been asked yet.
        let asked_session = asked.is_none();
        self.leaf()
            .update(cx, |pane, _| pane.asked_session = asked_session);
        self.show_pane(Kind::Session, cx);
        if let Some(entry) = asked {
            self.workspace
                .update(cx, |workspace, cx| workspace.new_session(entry, None, cx));
        }
    }

    /// Open a session on the agent the menu named, if it is still installed.
    pub(crate) fn new_session_with_action(
        &mut self,
        action: &NewSessionWith,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self
            .workspace
            .read(cx)
            .settings
            .agents
            .iter()
            .position(|agent| agent.name == action.agent);
        if let Some(at) = at {
            self.pick_agent(at, window, cx);
        }
    }

    /// Open a session on the agent after the one ⌘N would pick, wrapping — with
    /// two installed, that is always the other one.
    ///
    /// With none installed it is ⌘N, which is what says so.
    pub(crate) fn new_session_next_action(
        &mut self,
        _: &NewSessionNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.read(cx);
        let agents = &workspace.settings.agents;
        if agents.is_empty() {
            return self.new_session_action(&NewSession, window, cx);
        }
        let at = workspace
            .preferred_agent(cx)
            .and_then(|preferred| agents.iter().position(|agent| agent.name == preferred.name))
            .map_or(0, |ix| (ix + 1) % agents.len());
        self.pick_agent(at, window, cx);
    }

    /// Copy what the transcript has selected. Bound app-wide and reached only
    /// where nothing nearer to the focus claimed the chord.
    ///
    /// Only while the transcript is the pane in front. The selection belongs to
    /// the active session whichever pane is showing, so without this a ⌘C over
    /// a board copies a run out of a chat nobody is looking at — which reads as
    /// the chord doing nothing, right up until it is pasted.
    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if self.showing(cx) != Some(Kind::Session) {
            return;
        }
        self.copy_transcript_selection(cx);
    }

    pub(crate) fn next_entry(
        &mut self,
        _: &NextEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_entry(1, window, cx);
    }

    pub(crate) fn prev_entry(
        &mut self,
        _: &PrevEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_entry(-1, window, cx);
    }

    /// Step to the next entry the project has open, wrapping at the ends — one
    /// ring over every kind, in the order the sidebar lists them, so a board
    /// standing alone still has the article above it for a neighbour.
    fn cycle_entry(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        // A pane holding tabs answers the chord for its own strip: the tabs
        // are what is in front of you, and stepping past them to the sidebar's
        // list would skip what the pane itself is holding.
        if self.cycle_tab(step, window, cx) {
            return;
        }
        // And an arrangement answers it for what it holds. The sidebar's list
        // is not what is in front of you there, and every entry the space
        // holds is left out of it — see [`Self::ungrouped`] — so stepping into
        // that list opens something else and leaves the space behind.
        if self.cycle_arranged(step, window, cx) {
            return;
        }
        // Nothing on screen is nothing to step from: the launch view is not an
        // entry, and its neighbour is not another one.
        let Some(pane) = self.showing(cx) else {
            return;
        };
        let workspace = self.workspace.read(cx);
        let Some(project) = workspace.active else {
            return;
        };
        let Some(open) = workspace.projects.get(project) else {
            return;
        };
        let showing = match pane {
            Kind::Session => open.active.map(Showing::Session),
            Kind::Board => open
                .board
                .and_then(|ix| Some(Showing::Board(open.boards.get(ix)?.id.clone()))),
            Kind::Article => open
                .article
                .and_then(|ix| Some(Showing::Article(open.articles.get(ix)?.id.clone()))),
            Kind::Table => open
                .table
                .and_then(|ix| Some(Showing::Table(open.tables.get(ix)?.key.clone()))),
            Kind::App => open
                .app
                .and_then(|ix| Some(Showing::App(open.apps.get(ix)?.name.clone()))),
        }
        .map(|showing| Row::Entry {
            project: open.path.clone(),
            showing,
        });
        // The divider is a line, not a landing.
        let ring: Vec<Row> = self
            .list(cx)
            .entries(project)
            .into_iter()
            .filter(|row| !matches!(row, Row::Archive(_)))
            .collect();
        let at = showing.and_then(|row| ring.iter().position(|entry| *entry == row));
        let Some(landing) = stepped(at, ring.len(), step).map(|ix| ring[ix].clone()) else {
            return;
        };
        self.open_row(&landing, window, cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal(&landing, cx));
    }

    /// Step the focused pane to the next of its own tabs, wrapping at the
    /// ends. Says whether it did — a pane holding one entry has no strip of
    /// its own, and the chord means the sidebar's list instead.
    fn cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(front) = self.leaf().read(cx).entry.clone() else {
            return false;
        };
        let stack = self.workspace.read(cx).stack_of(&front);
        if stack.len() < 2 {
            return false;
        }
        let Some(at) = stack.iter().position(|tab| *tab == front) else {
            return false;
        };
        let landing = (at as isize + step).rem_euclid(stack.len() as isize) as usize;
        let (Some(pane), Some(tab)) = (stack.first().cloned(), stack.get(landing).cloned()) else {
            return false;
        };
        self.show_tab(&pane, &tab, window, cx);
        true
    }

    /// Step the window to the next entry the open arrangement holds, wrapping
    /// at the ends — every tab across every pane, in the order they are laid
    /// out. Says whether the chord was the arrangement's, which is whenever one
    /// is open: a space holding a single entry answers it by staying put. The
    /// sidebar's list leaves the space and can land in another project
    /// altogether, so falling through to it is never what the chord meant.
    ///
    /// Panes alone are the chord beside this one — see [`Self::step_pane`].
    fn cycle_arranged(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let ring = match self.arrangement(cx) {
            Some(space) => space.entries(),
            None => return false,
        };
        if ring.len() < 2 {
            return true;
        }
        let at = self
            .leaf()
            .read(cx)
            .entry
            .clone()
            .and_then(|front| ring.iter().position(|member| *member == front))
            .unwrap_or(0);
        let landing = (at as isize + step).rem_euclid(ring.len() as isize) as usize;
        let Some(tab) = ring.get(landing).cloned() else {
            return true;
        };
        let stack = self.workspace.read(cx).stack_of(&tab);
        let pane = stack.first().cloned().unwrap_or_else(|| tab.clone());
        self.show_tab(&pane, &tab, window, cx);
        true
    }

    /// Leaving a project is the moment a half-written card has to be filed:
    /// the spot it points at belongs to the board being navigated away from.
    pub(crate) fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.library = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.select_project(ix, cx));
        self.land(cx);
    }

    /// Put the pane on what the project coming forward was last showing.
    ///
    /// Without this the pane is whatever the last project was read in, and
    /// [`Self::showing`] falls back through the four in a fixed order — so a
    /// project with a board open from earlier in the session lands on the
    /// board however recently the article beside it was read.
    fn land(&mut self, cx: &mut Context<Self>) {
        if let Some(kind) = self.workspace.read(cx).landing() {
            self.leaf().update(cx, |pane, _| pane.kind = kind);
        }
    }

    pub(crate) fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.close_project(ix, cx));
    }

    /// Land on a session, caret in its composer.
    ///
    /// The composer is drawn only over a chat it can send to, and focus on an
    /// element no frame draws is focus nowhere — so a session that cannot take
    /// a message leaves the focus where it was.
    pub(crate) fn select_session(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let member = self.workspace.read(cx).member_of_session(id, cx);
        if self.enter_member(member, window, cx) {
            return;
        }
        self.show_pane(Kind::Session, cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_session(id, cx));
        self.sync_composer(cx);
        self.reveal_applied_match(cx);
        if self
            .workspace
            .read(cx)
            .session(id, cx)
            .is_some_and(ChatSession::resumable)
        {
            window.focus(&self.composer_focus_handle(cx), cx);
        }
    }

    pub(crate) fn open_settings_action(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(Section::General, window, cx);
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    pub(crate) fn toggle_sidebar_action(
        &mut self,
        _: &ToggleSidebar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    /// The menu's Close Project. The sidebar names a project by the row it was
    /// pressed on; the menu bar has only the one in front.
    pub(crate) fn close_project_action(
        &mut self,
        _: &CloseProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.workspace.read(cx).active else {
            return;
        };
        self.close_project(ix, cx);
    }

    pub(crate) fn open_settings(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "desktop")]
        {
            let _ = window;
            let workspace = self.workspace.clone();
            self.settings_window = settings::open(workspace, self.settings_window, section, cx);
        }
        #[cfg(not(feature = "desktop"))]
        self.settings_sheet(section, window, cx);
    }

    pub(crate) fn open_project_action(
        &mut self,
        _: &OpenProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cfg!(not(feature = "desktop")) {
            self.desktop_only("Opening a project", window, cx);
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.open_project(path, cx));
            });
        })
        .detach();
    }

    /// Which pane is on screen, as against [`Self::pane`], which is the one
    /// asked for. They part when what it points at is gone — deleted, switched
    /// off, or in a project that has none open — and whatever the project does
    /// have stands in, so a launch lands on the entry it was left on rather
    /// than on an empty conversation. The sidebar reads this, not the request:
    /// a row lit for a pane nobody can see is the second selection the eye
    /// finds.
    ///
    /// `None` is the launch view. A pane exists only where something is open in
    /// it, so with nothing open there is no pane to name — least of all the
    /// chat, which under the shipped defaults is itself switched off.
    pub(crate) fn showing(&self, cx: &App) -> Option<Kind> {
        // A pane in a space is whatever its entry is, whether or not it has
        // been focused yet: `leaf.pane` is only written by the focus. A leaf
        // is one tab, so its entry is what it shows.
        if let Some(entry) = &self.leaf().read(cx).entry
            && let Some((_, showing)) = self.workspace.read(cx).showing_of(entry, cx)
        {
            return Some(showing.kind());
        }
        if self.has_pane(self.leaf().read(cx).kind, cx) {
            return Some(self.leaf().read(cx).kind);
        }
        Kind::ALL.into_iter().find(|&pane| self.has_pane(pane, cx))
    }

    /// Whether the active project has anything open in `pane` — what makes it
    /// a pane there is to show, as against one asked for.
    pub(crate) fn has_pane(&self, pane: Kind, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        match pane {
            // A pane with no session in it, where one was asked for and there
            // is no agent to open it on — [`Self::asked_session`]. Conditioned
            // on the agent as well as on the flag, so an install is all it
            // takes to put the pane back to what it is for.
            Kind::Session => {
                workspace.active_session(cx).is_some()
                    || (self.leaf().read(cx).asked_session
                        && workspace.preferred_agent(cx).is_none())
            }
            Kind::Board => workspace.active_board(cx).is_some(),
            Kind::Article => workspace.active_article().is_some(),
            Kind::Table => workspace.active_table().is_some(),
            Kind::App => workspace
                .active_project()
                .is_some_and(|open| open.app.is_some()),
        }
    }

    /// Nothing is open, so there is nowhere to send a prompt — the only thing
    /// on offer is a folder.
    pub(crate) fn no_project(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        theme
            .empty_state(
                icons::files::Folder,
                "No project open",
                "An agent runs in a directory. Pick one to start.",
            )
            .flex_1()
            .child(
                theme
                    .button(
                        "Open folder…",
                        ButtonStyle::Prominent,
                        Some(Fade::new(painter, "open-project-empty")),
                    )
                    .id("open-project-empty")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_project_action(&OpenProject, window, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Render for Cydonia {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.track_history(cx);
        self.sync_leaves(window, cx);
        self.sync_changes(window, cx);
        self.sync_items(cx);
        self.sync_focus(cx);
        self.sync_panes(cx);
        self.sync_sidebar(cx);
        self.publish_shown(cx);
        self.hold_drawn_tables(cx);
        let theme = Theme::of(cx).clone();
        let root = div()
            .key_context("Cydonia")
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_style(TextStyle::Body)
            // The mouse's own back and forward buttons.
            .on_mouse_down(
                gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(|this, _, window, cx| this.go_back(&GoBack, window, cx)),
            )
            .on_mouse_down(
                gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(|this, _, window, cx| this.go_forward(&GoForward, window, cx)),
            )
            .on_action(cx.listener(|this, at: &SwitchSpace, window, cx| {
                let workspace = this.workspace.read(cx);
                let last = workspace.spaces.len().checked_sub(1);
                let Some(ix) = at.index.or(last).filter(|ix| Some(*ix) <= last) else {
                    return;
                };
                if workspace.space != Some(ix) {
                    this.open_space(ix, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextPane, window, cx| this.step_pane(1, window, cx)))
            .on_action(cx.listener(|this, _: &PrevPane, window, cx| this.step_pane(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &ClosePane, window, cx| this.close_focused_pane(window, cx)),
            )
            // `cmd-w` closes the tab in front, and the window where there is no
            // tab to close. Taken on the action rather than on the chord: the
            // window's binding is contextless, which `Keymap::binding_enabled`
            // ranks at the depth of the whole stack and so above every scoped
            // binding — a `cmd-w` claimed for a pane is outranked wherever
            // anything inside the pane holds the focus, which is everywhere
            // worth closing a tab from. An element handler runs before the
            // global one that removes the window, so this is where the two
            // meanings part.
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                match this.leaf().read(cx).entry.is_some() {
                    true => this.close_focused_pane(window, cx),
                    false => window.remove_window(),
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomPane, _, cx| this.zoom_focused_pane(cx)))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::commit_info))
            .on_action(cx.listener(Self::dismiss_info))
            // Everything the menu bar names, and only under the conditions
            // that keep its items honest.
            .map(|root| self.commands(root, cx))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width = f32::from(event.event.position.x)
                        .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
                    cx.notify();
                }),
            )
            // An action reaches the handlers above only through the focused
            // element's ancestors. Sized at nothing, so the pane that does hold
            // a field keeps its focus through a click anywhere else.
            .child(div().track_focus(&self.focus))
            .when(self.sidebar_open, |root| {
                root.child(self.sidebar_column(window, cx))
            })
            .child(self.detail(window, cx))
            // Rides on the seam between the sidebar and the detail column
            // rather than sitting in flow, so neither gives up a column.
            .when(self.sidebar_open, |root| {
                root.child(
                    crate::view::component::divider::divider(&theme, Axis::Horizontal)
                        .id("sidebar-split")
                        .absolute()
                        .top_0()
                        .left(px(
                            self.sidebar_width - crate::view::component::divider::HIT / 2.
                        ))
                        .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
                )
            })
            .children(
                self.workspace
                    .read(cx)
                    .meter
                    .then(|| meter::panel("app-meter", &self.meter_at, &self.meter, window)),
            )
            // Over every column and every floating control: nothing behind it
            // is answerable while it is asking.
            .child(self.modals.clone());
        let surface = self.dock.surface("dock", root).on_drop({
            let this = cx.entity().downgrade();
            move |event, window, cx| {
                this.update(cx, |this, cx| this.dock_drop(event, window, cx))
                    .ok()
                    .flatten()
            }
        });
        bezel::ui::window::frame(surface, window, cx)
    }
}
