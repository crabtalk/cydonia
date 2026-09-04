//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

use crate::{
    model::{session::ChatSession, settings::Settings, state::State, workspace::Workspace},
    view::{
        board::{self, Editing},
        component::{
            composer::{Composer, ComposerEvent},
            menu::Menu,
            meter,
        },
        settings::{self, Section, SettingsWindow},
        sidebar::{Filter, Renaming, Row},
        table,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, FocusHandle, Hsla,
        KeyBinding, PathPromptOptions, Render, UniformListScrollHandle, Window, WindowHandle,
        actions, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{Material, TextStyle, Theme, Typeset},
    ui::{
        floating::Floating,
        icons,
        input::TextField,
        stats::Stats,
        widgets::{ButtonStyle, Buttons, Content, Layout, SPLIT_HANDLE_HIT, SplitDrag, SplitStyle},
    },
};

actions!(
    cydonia,
    [
        NewSession,
        OpenProject,
        OpenSettings,
        CommitName,
        DismissName,
        NextEntry,
        PrevEntry
    ]
);

/// Claimed on the rename field so `enter` files the name and `escape` drops it.
const RENAME_CONTEXT: &str = "CydoniaSessionName";

const SIDEBAR_WIDTH: f32 = 200.;
const SIDEBAR_WIDTH_MIN: f32 = 180.;
const SIDEBAR_WIDTH_MAX: f32 = 420.;

/// The sidebar's gutter: a row's outer margin, and the padding inside it.
pub(crate) const SIDEBAR_GUTTER: f32 = 8.;

/// How thick each column's material sits. Nothing paints beneath them, so these
/// are absolute and independent: the sidebar is chrome and holds no long-form
/// text, the panel is the column whose text has to win against the desktop.
const SIDEBAR_MATERIAL: Material = Material::Thick;
const CONTENT_MATERIAL: Material = Material::UltraThick;

/// The header strip's height, measured off `../desktop`: between Cursor's 34
/// and Notion's 36, and tall enough to hold the 14px traffic lights macOS 26
/// draws without crowding them.
pub(crate) const HEADER_HEIGHT: f32 = 36.;

/// The pill at rest, and the agent mark beside it. Half of it is the stadium's
/// radius.
pub(crate) fn composer_height() -> f32 {
    composer_disc() + 2. * COMPOSER_INSET
}

/// The room the pill keeps around its content — the same 6 the height counts
/// above and below the line box.
pub(crate) const COMPOSER_INSET: f32 = 6.;

/// The send disc, filling the pill inside that inset, which lands it on the
/// line box it sits beside — the field's own box, so the two stay one height
/// wherever the text-size setting puts it.
pub(crate) fn composer_disc() -> f32 {
    TextStyle::Body.painted_line_height()
}

/// How far the floating composer stands off the column's bottom edge.
pub(crate) const COMPOSER_BOTTOM: f32 = 20.;

/// The sidebar's fill. Opaque, it takes the chrome tone: the light palette's
/// `surface` is the grey the content plane's white sits inside, and falling
/// back to the panel would leave the two columns one flat sheet.
pub(crate) fn sidebar_bg(theme: &Theme) -> Hsla {
    material(theme, SIDEBAR_MATERIAL).unwrap_or(theme.surface)
}

/// The content column's fill.
pub(crate) fn content_bg(theme: &Theme) -> Hsla {
    material(theme, CONTENT_MATERIAL).unwrap_or(theme.bg)
}

/// A column's own tint at one thickness on the material ladder, or nothing
/// where the window shows no desktop to sit over. The ladder's tone is a
/// neutral scrim and carries no appearance — tinting it is what makes dark
/// glass dark.
fn material(theme: &Theme, thickness: Material) -> Option<Hsla> {
    theme.vibrancy.then(|| Hsla {
        a: thickness.opacity(),
        ..theme.vibrancy_tint()
    })
}

/// macOS traffic light diameter — AppKit owns the buttons and reports their
/// frame, so nothing here can derive it. Measured on macOS 26.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;

/// Where the traffic lights go, for `TitlebarOptions::traffic_light_position`:
/// AppKit's own inset across, which is where every other window on the desktop
/// shows them, and down by half the band the header reserves for them. macOS
/// sizes the button container to `height + 2y`.
pub const TRAFFIC_LIGHT_X: f32 = 12.;
pub const TRAFFIC_LIGHT_Y: f32 = (HEADER_HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.;

/// Between the lights' centres, as AppKit lays them out. Measured on macOS 26.
const TRAFFIC_LIGHT_SPACING: f32 = 23.;

/// The gap the header keeps at the window's edges, and between the lights and
/// the first control it puts past them.
pub(crate) const HEADER_INSET: f32 = 16.;

/// Where the toolbar's own controls start: clear of the three lights AppKit
/// puts down from [`TRAFFIC_LIGHT_X`], plus the gutter that clears them and the
/// strip's own inset, so the first control stands off the lights by the same
/// measure it keeps from every other edge.
pub(crate) const TOOLBAR_INSET: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_LIGHT_X + 2. * TRAFFIC_LIGHT_SPACING + TRAFFIC_LIGHT_SIZE + 6. + HEADER_INSET
} else {
    HEADER_INSET
};

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewSession, None),
        KeyBinding::new("cmd-o", OpenProject, None),
        // What macOS binds Preferences to in every other app.
        KeyBinding::new("cmd-,", OpenSettings, None),
        // What a browser binds its tabs to. Global, because the point is to
        // move between documents without taking the hand out of the editor —
        // where `tab` itself is indent.
        KeyBinding::new("ctrl-tab", NextEntry, None),
        KeyBinding::new("ctrl-shift-tab", PrevEntry, None),
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]);
}

/// Which pane the detail column shows. A property of the window, not of a
/// project — switching projects must not teleport you to another pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Board,
    Article,
    Table,
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

/// The root view. It owns no app state — only the chrome's own: how wide the
/// sidebar is, which pane is showing, and whichever card is being written.
pub struct Cydonia {
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_width: f32,
    pub(crate) composer: Entity<Composer>,
    settings_window: Option<WindowHandle<SettingsWindow>>,
    pub(crate) pane: Pane,
    pub(crate) editing: Option<Editing>,
    pub(crate) card_field: Entity<TextField>,
    /// What the table pane's field is attached to, and the field itself.
    pub(crate) cell: Option<table::Cell>,
    pub(crate) cell_field: Entity<TextField>,
    pub(crate) menu: Option<Menu>,
    /// Whether the press now being handled landed on the open menu's own
    /// trigger — read by [`Cydonia::toggle_menu`] and nothing else.
    pub(crate) menu_pressed: bool,
    /// Which kinds the sidebar is listing.
    pub(crate) filter: Filter,
    /// What the name field is attached to, and the field itself.
    pub(crate) renaming: Option<Renaming>,
    pub(crate) name_field: Entity<TextField>,
    meter: Entity<Stats>,
    meter_at: Floating,
    /// The rail's scroll. A step taken from the keyboard has to bring its
    /// landing into view; the list does not scroll itself.
    pub(crate) rail: UniformListScrollHandle,
    /// Where the focus rests when no field holds it — a board, a table and a
    /// transcript have none — so the bindings below always have a path here.
    focus: FocusHandle,
}

impl Cydonia {
    pub fn new(
        settings: Settings,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(Composer::new);
        cx.subscribe(
            &composer,
            |this, _, event: &ComposerEvent, cx| match event {
                ComposerEvent::Submit(text) => this.submit(text.clone(), cx),
                ComposerEvent::Cancel => this.cancel_turn(cx),
                ComposerEvent::Agent(ix) => this.pick_agent(*ix, cx),
                ComposerEvent::Install => this.open_settings(Section::Agents, cx),
            },
        )
        .detach();

        let card_field = board::field(cx);
        let cell_field = table::field(cx);
        let name_field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_key_context(RENAME_CONTEXT)
                .with_placeholder("name this session…")
        });
        let workspace = cx.new(|cx| Workspace::new(settings, state, cx));
        // The model is the only thing that says a session appeared or a turn
        // ended; the composer's placeholder, commands and busy state are all
        // read back from it rather than pushed by whoever caused the change.
        cx.observe(&workspace, |this, _, cx| this.sync_composer(cx))
            .detach();

        let mut this = Self {
            meter: cx.new(Stats::new),
            meter_at: Floating::new(Painter::of(cx)),
            workspace,
            sidebar_open: true,
            sidebar_width: SIDEBAR_WIDTH,
            composer,
            settings_window: None,
            pane: Pane::Chat,
            editing: None,
            card_field,
            cell: None,
            cell_field,
            menu: None,
            menu_pressed: false,
            filter: Filter::default(),
            renaming: None,
            name_field,
            rail: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
        };
        // Whatever held the focus has left the tree — the composer with the
        // chat pane, an editor with its article — and an unrendered element
        // dispatches nothing, so the window takes its focus back.
        cx.on_focus_lost(window, |this, window, cx| window.focus(&this.focus, cx))
            .detach();
        this.sync_composer(cx);
        // Where the caret starts. The composer is drawn only over a chat it can
        // send to, and focus on an element no frame draws is focus nowhere.
        let composer = this
            .workspace
            .read(cx)
            .active_session()
            .is_some_and(ChatSession::resumable)
            .then(|| this.composer_focus_handle(cx));
        window.focus(composer.as_ref().unwrap_or(&this.focus), cx);
        this
    }

    pub(crate) fn new_session_action(
        &mut self,
        _: &NewSession,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_pane(Pane::Chat, cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(entry) = workspace.preferred_agent() {
                workspace.new_session(entry, None, cx);
            }
        });
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
        let pane = self.showing(cx);
        let workspace = self.workspace.read(cx);
        let Some(project) = workspace.active else {
            return;
        };
        let Some(open) = workspace.projects.get(project) else {
            return;
        };
        let showing = match pane {
            Pane::Chat => open.active.map(|id| Row::Session { project, id }),
            Pane::Board => open.board.map(|ix| Row::Board { project, ix }),
            Pane::Article => open.article.map(|ix| Row::Article { project, ix }),
            Pane::Table => open.table.map(|ix| Row::Table { project, ix }),
        };
        // The divider is a line, not a landing.
        let ring: Vec<Row> = self
            .entries(project, cx)
            .into_iter()
            .filter(|row| !matches!(row, Row::Archive(_)))
            .collect();
        let at = showing.and_then(|row| ring.iter().position(|entry| *entry == row));
        let Some(landing) = stepped(at, ring.len(), step).map(|ix| ring[ix]) else {
            return;
        };
        self.open_row(landing, window, cx);
        self.reveal(landing, cx);
    }

    /// Leaving a project is the moment a half-written card has to be filed:
    /// the spot it points at belongs to the board being navigated away from.
    pub(crate) fn select_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_project(ix, cx));
    }

    pub(crate) fn close_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.close_project(ix, cx));
    }

    pub(crate) fn select_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.show_pane(Pane::Chat, cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_session(id, cx));
    }

    fn open_settings_action(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(Section::Appearance, cx);
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    pub(crate) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        self.settings_window = settings::open(workspace, self.settings_window, section, cx);
    }

    pub(crate) fn open_project_action(
        &mut self,
        _: &OpenProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    /// asked for. They part when what it points at is gone — deleted, or in a
    /// project that has none open — and whatever the project does have stands
    /// in, so a launch lands on the entry it was left on rather than on an
    /// empty conversation. The sidebar reads this, not the request: a row lit
    /// for a pane nobody can see is the second selection the eye finds.
    pub(crate) fn showing(&self, cx: &App) -> Pane {
        let open = |pane| {
            let workspace = self.workspace.read(cx);
            match pane {
                Pane::Chat => workspace.active_session().is_some(),
                Pane::Board => workspace.active_board().is_some(),
                Pane::Article => workspace.active_article().is_some(),
                Pane::Table => workspace.active_table().is_some(),
            }
        };
        if open(self.pane) {
            return self.pane;
        }
        [Pane::Chat, Pane::Board, Pane::Article, Pane::Table]
            .into_iter()
            .find(|&pane| open(pane))
            .unwrap_or(Pane::Chat)
    }

    /// Nothing is open, so there is nowhere to send a prompt — the only thing
    /// on offer is a folder.
    pub(crate) fn no_project(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        theme
            .empty_state(
                icons::FOLDER,
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
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_style(TextStyle::Body)
            .on_action(cx.listener(Self::new_session_action))
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::open_settings_action))
            .on_action(cx.listener(Self::next_entry))
            .on_action(cx.listener(Self::prev_entry))
            .on_action(cx.listener(Self::commit_name))
            .on_action(cx.listener(Self::dismiss_name))
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
            .when(self.sidebar_open, |root| root.child(self.sidebar(cx)))
            .child(self.detail(window, cx))
            // Rides on the seam between the sidebar and the detail column
            // rather than sitting in flow, so neither gives up a column.
            .when(self.sidebar_open, |root| {
                root.child(
                    theme
                        .split_handle(Axis::Horizontal, SplitStyle::Ghost)
                        .id("sidebar-split")
                        .absolute()
                        .top_0()
                        .left(px(self.sidebar_width - SPLIT_HANDLE_HIT / 2.))
                        .on_drag(SplitDrag, |_, _, _, cx| cx.new(|_| Empty)),
                )
            })
            .children(
                self.workspace
                    .read(cx)
                    .meter
                    .then(|| meter::panel("app-meter", &self.meter_at, &self.meter, window)),
            )
    }
}
