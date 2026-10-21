//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

#[cfg(feature = "desktop")]
use crate::view::{component::terminal::TerminalPanel, pane, settings};
use crate::{
    model::{
        session::ChatSession,
        settings::Settings,
        state::State,
        workspace::{Opening, Reloaded, Showing, Slot, Workspace},
    },
    view::{
        board,
        component::{
            composer::{Composer, ComposerEvent},
            meter,
        },
        kit::{menu::Menu, watch::Watch},
        pane::{Pane, Tab},
        sidebar::{Dragged, Row, Sidebar},
        window::{
            chrome,
            lights::{self, Lights},
            menubar::CloseWindow,
            modal::ModalLayer,
            section::Section,
        },
    },
};
use anyhow::Result;
use artifact::{kind::Kind, space::Member};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Bounds, Context, Div, DragMoveEvent, Empty, Entity,
        FocusHandle, Focusable, KeyBinding, PathPromptOptions, Pixels, Point, Render,
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
use std::{cell::RefCell, rc::Rc};

pub(crate) const SIDEBAR_WIDTH: f32 = 200.;

const SIDEBAR_WIDTH_MIN: f32 = 180.;

const SIDEBAR_WIDTH_MAX: f32 = 420.;

/// The sidebar's gutter: a row's outer margin, and the padding inside it.
pub(crate) const SIDEBAR_GUTTER: f32 = 8.;

/// The window's top row — the strip of tabs and the sidebar's band — which
/// the traffic lights centre in down its height. At this height they stand
/// where AppKit lays them out itself in a transparent, full-size-content
/// titlebar on macOS 26. AppKit puts them back there through a fullscreen
/// transition before gpui moves them, so anywhere else they jump.
pub(crate) const HEADER_HEIGHT: f32 = 32.;

/// A pane's title row and the right panel's tab row, away from the window's
/// top.
pub(crate) const CARD_ROW_HEIGHT: f32 = 32.;

/// The room the pill keeps above and below its content.
pub(crate) const COMPOSER_INSET: f32 = 4.;

/// How far the floating composer stands off the column's bottom edge.
pub(crate) const COMPOSER_BOTTOM: f32 = 20.;

/// How wide the composer's column runs before it stops growing, and the air it
/// keeps either side of itself inside that.
pub(crate) const COMPOSER_COLUMN: f32 = 720.;

pub(crate) const COMPOSER_MARGIN: f32 = 24.;

/// How far the close button stands in from the window's left edge, as far as
/// the lights stand down from its top: concentric with the window's corner,
/// as macOS 26 places them.
pub(crate) const TRAFFIC_LIGHT_INSET: f32 =
    (HEADER_HEIGHT - crate::view::window::lights::LIGHT_SIZE) / 2.;

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

/// The padding at the ends of a card's own row — a pane's title row, the
/// right panel's tabs — before the icon button there.
pub(crate) const CARD_EDGE: f32 = 4.;

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
        InstallApp,
        LoadApp,
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
        NextInStack,
        PrevInStack,
        ClosePane,
        ZoomPane,
        CopySelection,
        GoBack,
        GoForward,
        WhatsNew
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

/// Bring the tab at `index` of the window's strip forward — `cmd-1` through
/// `cmd-8` — or its last for `None`, `cmd-9`. An index past the last tab does
/// nothing.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = cydonia, no_json)]
pub struct SwitchTab {
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

/// The chrome between two floating cards, and between a card and the
/// sidebar: a seam stands in its middle. Nothing where the content is flat.
pub(crate) fn card_gap(floating: bool) -> f32 {
    match floating {
        true => Theme::CONTENT_INSET,
        false => 0.,
    }
}

/// The lights' origin, for `TitlebarOptions::traffic_light_position`, until
/// [`lights::fit`] has measured them.
pub(crate) fn traffic_lights() -> Point<Pixels> {
    point(px(TRAFFIC_LIGHT_INSET), px(TRAFFIC_LIGHT_INSET))
}

/// The band across the top of a column, and the only place its height and its
/// inset are written: the window's band over the library and the launch view,
/// the strip, the sidebar's and the right panel's are all this row.
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

/// Where the toolbar's own controls start at the window's left edge, under a
/// top band `band` tall: the button whose glyph stands [`LIGHTS_GAP`] past the
/// green light, or in full screen, where there are no lights, in line with
/// the sidebar rows' icons.
pub(crate) fn toolbar_inset(window: &Window, band: f32, cx: &App) -> f32 {
    match (window.is_fullscreen(), cfg!(target_os = "macos")) {
        (true, _) => 2. * SIDEBAR_GUTTER - BUTTON_SLACK,
        (false, true) => Lights::of(cx).end(band) + LIGHTS_GAP - BUTTON_SLACK,
        (false, false) => HEADER_INSET,
    }
}

/// The chords the window keeps whatever the reader says — the commands it also
/// answers to are bound from [`crate::view::keymap`], which is where they can
/// be moved.
pub fn bindings() -> Vec<KeyBinding> {
    let tabs = (1..=9).map(|n| {
        KeyBinding::new(
            &format!("secondary-{n}"),
            SwitchTab {
                index: (n < 9).then(|| n - 1),
            },
            Some("Cydonia"),
        )
    });
    tabs.chain([
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
        // The same chords, taken by a pane holding a stack while the focus is
        // in it.
        KeyBinding::new("ctrl-tab", NextInStack, Some(pane::STACK_CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PrevInStack, Some(pane::STACK_CONTEXT)),
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
            // No titlebar of its own: the traffic lights sit in the window's
            // top row.
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
            lights::fit(window, HEADER_HEIGHT, cx);
            cx.new(|cx| {
                let mut root = Cydonia::new(settings, state, window, cx);
                root.restore_panel_layout();
                cx.defer_in(window, |root: &mut Cydonia, window, cx| {
                    root.whats_new_once(window, cx)
                });
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
            let _ = window.update(cx, |root, window, cx| {
                root.select_session(id, Opening::Preview, window, cx)
            });
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
    /// A tab for every entry the open space holds and every entry of the
    /// window's strip, and which of them has the focus. Empty where no tab is
    /// open.
    pub(crate) leaves: Vec<Entity<Tab>>,
    /// The panes of what the window lays out — see [`Pane`].
    pub(crate) panes: Vec<Entity<Pane>>,
    /// The window's strip of tabs — see [`crate::view::window::strip`].
    pub(crate) strip: Entity<crate::view::window::strip::Strip>,
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
    /// The tab the last press came down in: `None` for the library. Where a
    /// link opens — see [`Cydonia::open_reference`].
    pub(crate) pressed_pane: Option<Member>,
    /// The modal over the window, while one is up.
    pub(crate) modals: Entity<ModalLayer>,
    /// Where a sidebar row or a pane's tab lands on a pane. The panes are its
    /// targets, by the member in front of each; the window is its surface.
    pub(crate) dock: docking::Dock<Member, Dragged>,
    /// Every board on screen: cards between lanes and boards, lanes and list
    /// groups along their own board.
    pub(crate) board_sort: drag::Domain<board::BoardRegion, board::BoardItem>,
    /// The tabs that have been brought to the front, most recent last, across
    /// every pane. Read when a pane's last tab closes with the focus on it —
    /// see [`Cydonia::close_pane`]. Runtime only.
    pub(crate) tab_history: Vec<Member>,
    /// A zoom on its way — see [`Cydonia::toggle_zoom`].
    pub(crate) zoom_glide: Option<crate::view::pane::arrangement::ZoomGlide>,
    pub(crate) pane_places: Rc<RefCell<crate::view::pane::arrangement::Places>>,
    /// The library, while it stands over the panes — see
    /// [`crate::view::library`].
    pub(crate) library: Option<Entity<crate::view::library::Library>>,
    /// The menu that is up, of every menu the window's screens open — see
    /// [`crate::view::kit::menu`].
    pub(crate) menu: ContextMenu<Cydonia, Menu>,
    /// The sidebar's search across projects — see [`crate::view::search`].
    pub(crate) search: Entity<crate::view::search::Search>,
    meter: Entity<Stats>,
    meter_at: Floating,
    /// Where the focus rests when no field holds it — a board, a table and a
    /// transcript have none — so the bindings below always have a path here.
    pub(crate) focus: FocusHandle,
    /// The top band the lights were last centred down — see [`lights::fit`].
    lights_band: f32,
}

impl Cydonia {
    /// The tab with the focus — what a command without a tab of its own acts
    /// on, and what the sidebar lights. Nothing where no tab is open.
    pub(crate) fn leaf(&self) -> Option<&Entity<Tab>> {
        let at = self.focused.min(self.leaves.len().saturating_sub(1));
        self.leaves.get(at)
    }

    /// The entry the focused tab is on.
    pub(crate) fn focused_entry(&self, cx: &App) -> Option<Member> {
        Some(self.leaf()?.read(cx).entry.clone())
    }

    /// What the model saying something changed asks of the window: the applied
    /// search read again, and the composers pointed at what they are on.
    fn model_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.watch_sessions(window, cx);
        self.search.update(cx, crate::view::search::Search::refresh);
        let session = |this: &Self, cx: &App| {
            this.leaf()
                .and_then(|leaf| leaf.read(cx).composer.read(cx).session())
        };
        let previous = session(self, cx);
        self.sync_composer(cx);
        let current = session(self, cx);
        // An article's composer takes the focus only from a press into it.
        if current.is_some()
            && current != previous
            && self
                .leaf()
                .is_some_and(|leaf| leaf.read(cx).thread_article(cx).is_none())
            && let Some(focus) = self.composer_focus_handle(cx)
        {
            window.focus(&focus, cx);
        }
    }

    /// Observe every session the workspace holds: the composers follow each
    /// one's own change.
    fn watch_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chats = self.workspace.read(cx).chat_entities();
        let mut watches = std::mem::take(&mut self.session_watches);
        watches.set_in(&chats, window, cx, |this, _, window, cx| {
            this.model_changed(window, cx)
        });
        self.session_watches = watches;
    }

    /// The sidebar's column: the band at its top, where the window's corner
    /// controls stand and which the window is dragged by, over the sidebar.
    fn sidebar_column(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            // Floating, the root paints the chrome under the sidebar and the
            // card alike.
            .when(!floating, |column| column.bg(theme.sidebar_bg()))
            .flex()
            .flex_col()
            .child(chrome::top_band(self.tabs_on_top(cx)).child(chrome::grip(
                "sidebar-grip",
                &self.drag,
                window,
            )))
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

    /// Tell the strip where it stands against the window's chrome.
    fn sync_strip(&mut self, cx: &mut Context<Self>) {
        let sidebar_open = self.sidebar_open;
        self.strip
            .update(cx, |strip, cx| strip.seat(sidebar_open, cx));
    }

    /// Tell each pane whether it is the one with the focus, where that moved.
    pub(crate) fn sync_focus(&mut self, cx: &mut Context<Self>) {
        let Some(focused) = self.leaf().map(|leaf| leaf.entity_id()) else {
            return;
        };
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

    /// A tab on `entry`, its composer wired to this window.
    fn new_pane(&self, entry: Member, window: &mut Window, cx: &mut Context<Self>) -> Entity<Tab> {
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
                if !matches!(event, ComposerEvent::Draft(..)) {
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

    /// Hand the tools what is on screen — see [`mcp::rail::shown`].
    fn publish_shown(&self, cx: &App) {
        // A session is the chat the tools are talking in, not a thing shown.
        let kind = |kind: Kind| (kind != Kind::Session).then_some(kind);
        let fronts = self
            .arrangement(cx)
            .map(|space| space.tree.fronts())
            .unwrap_or_default();
        let shown: Vec<mcp::rail::Shown> = self
            .leaves
            .iter()
            .enumerate()
            .filter(|(_, leaf)| fronts.contains(&leaf.read(cx).entry))
            .filter_map(|(ix, leaf)| {
                let entry = leaf.read(cx).entry.entry()?;
                Some(mcp::rail::Shown {
                    project: entry.project.clone(),
                    kind: kind(entry.kind)?,
                    id: entry.id.clone(),
                    focused: ix == self.focused,
                })
            })
            .collect();
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

    /// Reconcile the tabs with what the window holds: one for every entry
    /// the arrangement in front holds, behind a pane's front included, in its
    /// order, then one for every other entry of the strip.
    ///
    /// A tab already on an entry is kept, so switching tabs, spaces or a
    /// stack's front does not throw away a composer with a draft in it.
    pub(crate) fn sync_leaves(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut members = self
            .arrangement(cx)
            .map(|space| space.entries())
            .unwrap_or_default();
        for member in self.workspace.read(cx).tab_strip() {
            if !members.contains(&member) {
                members.push(member);
            }
        }
        let focused = self.focused_entry(cx);
        let mut kept: Vec<Entity<Tab>> = Vec::with_capacity(members.len());
        for entry in &members {
            match self
                .leaves
                .iter()
                .position(|leaf| leaf.read(cx).entry == *entry)
            {
                Some(at) => kept.push(self.leaves.remove(at)),
                None => kept.push(self.new_pane(entry.clone(), window, cx)),
            }
        }
        self.leaves = kept;
        self.focused = focused
            .and_then(|on| {
                self.leaves
                    .iter()
                    .position(|leaf| leaf.read(cx).entry == on)
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
    /// opened and the pane focused; under a project, the entry opens in the
    /// window's strip — see [`Workspace::open_tab`]. Where the window happened
    /// to be standing does not enter into it.
    ///
    pub(crate) fn enter_member(
        &mut self,
        member: Member,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.library = None;
        match self.workspace.read(cx).space_holding(&member) {
            Some(at) => {
                if self.workspace.read(cx).space() != Some(at) {
                    self.open_space(at, window, cx);
                }
                self.workspace
                    .update(cx, |workspace, cx| workspace.set_front(&member, cx));
            }
            None => self.workspace.update(cx, |workspace, cx| {
                workspace.open_tab(member.clone(), opening, cx);
            }),
        }
        self.show_pane(&member, window, cx);
    }

    /// Bring a tab of the strip forward, and the focus with it.
    pub(crate) fn show_slot(&mut self, slot: &Slot, window: &mut Window, cx: &mut Context<Self>) {
        self.library = None;
        match slot {
            Slot::Space { space } => {
                if let Some(ix) = self.workspace.read(cx).space_ix(space) {
                    self.open_space(ix, window, cx);
                }
            }
            Slot::Entry(member) => {
                self.workspace
                    .update(cx, |workspace, cx| workspace.front_tab(slot, cx));
                self.refocus(member, window, cx);
            }
        }
        cx.notify();
    }

    /// Bring the leaves up to the arrangement and move the focus to the pane
    /// on `entry`, putting the project's selection on it even where that pane
    /// is already the focused leaf.
    pub(crate) fn refocus(&mut self, entry: &Member, window: &mut Window, cx: &mut Context<Self>) {
        // A pane the arrangement has gained since the last frame has no leaf
        // yet, and [`Cydonia::focus_pane`] moves nothing it cannot find.
        self.sync_leaves(window, cx);
        self.focused = usize::MAX;
        self.focus_pane(entry, window, cx);
        cx.notify();
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
            .position(|leaf| leaf.read(cx).entry == *entry)
        else {
            // [`Cydonia::refocus`] parks the index past the end to force the
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
        self.workspace
            .update(cx, |workspace, _| workspace.set_focused(entry));
        let Some(leaf) = self.leaf().cloned() else {
            return;
        };
        // An app's fields take the caret when they are pressed.
        if let Member::App { .. } = entry {
            let here = leaf.read(cx).focus.clone();
            window.focus(&here, cx);
            cx.notify();
            return;
        }
        let Some((project, showing)) = self.workspace.read(cx).showing_of(entry, cx) else {
            // A member whose project is shut draws an empty pane, which takes
            // no caret — so the focus comes back to the window rather than
            // staying wherever it was.
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        };
        if moved {
            self.workspace.update(cx, |workspace, cx| {
                workspace.select_showing(project, showing.clone(), cx);
            });
        }
        self.sync_composer(cx);
        // The press already put the focus on something inside this pane — a
        // transcript's text, a field — before bubbling here. Moving it to the
        // composer would take ⌘C away from the run just selected.
        let here = leaf.read(cx).focus.clone();
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
                .then(|| self.composer_focus_handle(cx))
                .flatten(),
            Showing::Article(_) => self.pane_editor(cx).map(|editor| editor.focus_handle(cx)),
            // A board or a table takes no caret of its own, but an open card
            // or cell is a field inside one: pressing into the text a second
            // time is how the caret is moved, and settling on the pane instead
            // would take the field's focus off it every time.
            Showing::Board(id) => leaf
                .read(cx)
                .boards
                .iter()
                .map(|view| view.read(cx))
                .find(|view| view.board.id == *id)
                .filter(|view| view.editing.is_some())
                .map(|view| view.card_field.read(cx).focus_handle(cx)),
            Showing::Table(_) => self
                .table_view_of(Some(&leaf.read(cx).entry), cx)
                .map(|view| view.read(cx))
                .filter(|view| view.cell.is_some())
                .map(|view| view.cell_field.read(cx).focus_handle(cx)),
        };
        // The pane's own handle, not the window's: the root's is tracked on a
        // sibling of the panes, so landing there puts the focus outside the
        // pane and the chords the pane claims stop being reached.
        window.focus(caret.as_ref().unwrap_or(&here), cx);
        cx.notify();
    }

    /// The tab on this entry, falling back to the focused one where the
    /// caller has no entry to name.
    ///
    /// Drawing reads a tab's state through this rather than through
    /// [`Self::leaf`]: every tab drawn against the focused leaf shares one
    /// scroll and one editor between them, so moving the focus
    /// moves what the other tabs are showing.
    pub(crate) fn leaf_of(&self, on: Option<&Member>, cx: &App) -> Option<&Entity<Tab>> {
        match on {
            Some(on) => self.leaves.iter().find(|leaf| leaf.read(cx).entry == *on),
            None => self.leaf(),
        }
    }

    pub fn new(
        settings: Settings,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let strip = {
            let (workspace, root) = (workspace.clone(), cx.entity().downgrade());
            cx.new(|cx| crate::view::window::strip::Strip::new(workspace, root, cx))
        };
        // Whether the strip shows its tabs sets the height of the band the
        // window draws its corner controls and the sidebar's top in.
        cx.observe(&strip, |_, _, cx| cx.notify()).detach();
        let mut this = Self {
            strip,
            meter: cx.new(Stats::new),
            meter_at: Floating::new(Painter::of(cx)),
            workspace,
            leaves: Vec::new(),
            panes: Vec::new(),
            session_watches: Watch::default(),
            sidebar,
            focused: 0,
            lights_band: HEADER_HEIGHT,
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
            board_sort: drag::Domain::with_ghost({
                let this = cx.entity().downgrade();
                move |item, window, cx| board::ghost(&this, item, window, cx)
            }),
            tab_history: Vec::new(),
            zoom_glide: None,
            pane_places: Rc::default(),
            modals: cx.new(|_| ModalLayer::default()),
            library: None,
            menu: ContextMenu::default(),
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
        // The window comes back on the tabs it was left on.
        this.sync_leaves(window, cx);
        if let Some(front) = this.arrangement(cx).and_then(|space| space.focused_front())
            && let Some(at) = this
                .leaves
                .iter()
                .position(|leaf| leaf.read(cx).entry == front)
        {
            this.focused = at;
        }
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
            .then(|| this.composer_focus_handle(cx))
            .flatten();
        window.focus(composer.as_ref().unwrap_or(&this.focus), cx);
        this.watch_sessions(window, cx);
        this
    }

    /// Open a session on whichever agent the last one ran on, or on the first
    /// one configured, as a tab of its own.
    ///
    /// With none configured there is nothing to open a session *on*. A fresh
    /// install names no agent — see
    /// [`crate::model::settings::Settings::default`] — so this is where most
    /// people meet the feature: the notice it raises offers the section that
    /// installs one, the same place the composer's own `Install an agent…`
    /// goes.
    pub(crate) fn new_session_action(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(agent) = self.workspace.read(cx).preferred_agent(cx) else {
            // Nothing to open one on: the window says so and offers the
            // install, rather than jumping to Settings on its own.
            return self.no_agent(window, cx);
        };
        let id = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_session(agent, None, cx));
        if let Some(id) = id {
            self.select_session(id, Opening::Kept, window, cx);
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
        // With the spaces in the sidebar there are no tabs to step through:
        // the chord is the focused pane's stack's, and nothing else's.
        if self
            .workspace
            .read(cx)
            .settings
            .appearance
            .spaces_in_sidebar
        {
            self.step_stack(step, window, cx);
            return;
        }
        // The strip answers it while it holds more than one tab: the tabs are
        // what is in front of you.
        if self.cycle_strip(step, window, cx) {
            return;
        }
        // Nothing on screen is nothing to step from: the launch view is not an
        // entry, and its neighbour is not another one.
        let Some(entry) = self.focused_entry(cx) else {
            return;
        };
        let workspace = self.workspace.read(cx);
        let Some(project) = entry.project().and_then(|path| workspace.project_at(path)) else {
            return;
        };
        let showing = Row::of_member(&entry, workspace, cx);
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
        self.open_row(&landing, Opening::Preview, window, cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal(&landing, cx));
    }

    /// Step the strip to its next tab, wrapping at the ends. Says whether it
    /// did — a strip of one tab leaves the chord to the sidebar's list.
    fn cycle_strip(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let workspace = self.workspace.read(cx);
        let strip = workspace.strip();
        if strip.len() < 2 {
            return false;
        }
        let at = workspace
            .front_slot()
            .and_then(|front| strip.iter().position(|slot| slot == front));
        let Some(landing) = stepped(at, strip.len(), step).map(|ix| strip[ix].clone()) else {
            return false;
        };
        self.show_slot(&landing, window, cx);
        true
    }

    /// Leaving a project is the moment a half-written card has to be filed:
    /// the spot it points at belongs to the board being navigated away from.
    pub(crate) fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.library = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.select_project(ix, cx));
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
    pub(crate) fn select_session(
        &mut self,
        id: u64,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let member = self.workspace.update(cx, |workspace, cx| {
            workspace.select_session(id, cx);
            workspace.mint_record(id, cx)?;
            workspace.member_of_session(id, cx)
        });
        let Some(member) = member else {
            return;
        };
        self.enter_member(member, opening, window, cx);
        self.sync_composer(cx);
        self.reveal_applied_match(cx);
        if self
            .workspace
            .read(cx)
            .session(id, cx)
            .is_some_and(ChatSession::resumable)
            && let Some(focus) = self.composer_focus_handle(cx)
        {
            window.focus(&focus, cx);
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
            settings::open(self.workspace.clone(), section, cx);
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

    /// The kind of entry the focused tab is on. `None` where no tab is open,
    /// which is the launch view, or where its entry has gone.
    pub(crate) fn showing(&self, cx: &App) -> Option<Kind> {
        let entry = self.focused_entry(cx)?;
        match entry {
            Member::App { .. } => Some(Kind::App),
            Member::Entry(_) => self
                .workspace
                .read(cx)
                .showing_of(&entry, cx)
                .map(|(_, showing)| showing.kind()),
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
        let band = chrome::top_height(self.tabs_on_top(cx));
        if self.lights_band != band {
            lights::fit(window, band, cx);
            self.lights_band = band;
        }
        self.sync_leaves(window, cx);
        self.sync_changes(window, cx);
        self.sync_items(cx);
        self.sync_sidebar(cx);
        self.sync_strip(cx);
        self.sync_edges(cx);
        self.publish_shown(cx);
        self.hold_drawn_tables(cx);
        let theme = Theme::of(cx).clone();
        let floating = self.workspace.read(cx).settings.appearance.floating;
        let root = div()
            .key_context("Cydonia")
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .when(floating, |root| root.bg(theme.sidebar_bg()))
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
            .on_action(cx.listener(|this, at: &SwitchTab, window, cx| {
                let strip = this.workspace.read(cx).strip();
                let last = strip.len().checked_sub(1);
                let Some(slot) = at
                    .index
                    .or(last)
                    .filter(|ix| Some(*ix) <= last)
                    .and_then(|ix| strip.get(ix).cloned())
                else {
                    return;
                };
                this.show_slot(&slot, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextPane, window, cx| this.step_pane(1, window, cx)))
            .on_action(cx.listener(|this, _: &PrevPane, window, cx| this.step_pane(-1, window, cx)))
            .on_action(
                cx.listener(|this, _: &NextInStack, window, cx| this.step_stack(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PrevInStack, window, cx| this.step_stack(-1, window, cx)),
            )
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
            .on_action(cx.listener(
                |this, _: &CloseWindow, window, cx| match this.leaf().is_some() {
                    true => this.close_focused_pane(window, cx),
                    false => window.remove_window(),
                },
            ))
            .on_action(cx.listener(|this, _: &ZoomPane, _, cx| this.zoom_focused_pane(cx)))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::copy_selection))
            // Everything the menu bar names, and only under the conditions
            // that keep its items honest.
            .map(|root| self.commands(root, cx))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    let middle =
                        card_gap(this.workspace.read(cx).settings.appearance.floating) / 2.;
                    this.sidebar_width = (f32::from(event.event.position.x) - middle)
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
            // Floating, the seam is the chrome between them.
            .when(self.sidebar_open, |root| {
                use crate::view::component::divider::{HIT, seam};
                root.child(
                    seam(&theme, Axis::Horizontal, floating)
                        .id("sidebar-split")
                        .absolute()
                        .top_0()
                        .left(px(self.sidebar_width + card_gap(floating) / 2. - HIT / 2.))
                        .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
                )
            })
            .children(
                self.workspace
                    .read(cx)
                    .meter
                    .then(|| meter::panel("app-meter", &self.meter_at, &self.meter, window)),
            )
            .child(self.corner_controls(window, cx))
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
