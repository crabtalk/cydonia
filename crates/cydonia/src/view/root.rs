//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

use crate::{
    model::{
        session::ChatSession,
        settings::Settings,
        state::State,
        workspace::{Reloaded, Workspace},
    },
    view::{
        board::{self, Editing},
        component::{
            composer::{Composer, ComposerEvent},
            menu::Menu,
            meter,
        },
        header,
        settings::{self, Section, SettingsWindow},
        sidebar::{Filter, Renaming, Row},
        table,
    },
};
use anyhow::Result;
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Bounds, Context, DragMoveEvent, Empty, Entity, FocusHandle,
        Hsla, KeyBinding, PathPromptOptions, Render, TitlebarOptions, UniformListScrollHandle,
        Window, WindowBounds, WindowHandle, WindowOptions, actions, div, point, prelude::*, px,
        size,
    },
    motion::{Fade, Painter},
    theme::{Material, TextStyle, Theme, Typeset, appearance},
    ui::{
        floating::Floating,
        icons,
        input::TextField,
        menu::Cursor,
        stats::Stats,
        widgets::{ButtonStyle, Buttons, Content, Layout, SPLIT_HANDLE_HIT, SplitDrag, SplitStyle},
    },
};

actions!(
    cydonia,
    [
        NewSession,
        NewBoard,
        NewArticle,
        NewTable,
        OpenProject,
        CloseProject,
        OpenSettings,
        ToggleSidebar,
        ShowChat,
        ShowBoard,
        ShowArticle,
        ShowTable,
        CommitName,
        DismissName,
        NextEntry,
        PrevEntry,
        CopySelection
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
        // What every app with a sidebar binds it to. It is claimed app-wide:
        // the menu item carries it, so AppKit takes the chord before the
        // window is offered it, and the editor's own `cmd-b` — bold — is not
        // reached while this one is on the bar.
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-1", ShowChat, None),
        KeyBinding::new("cmd-2", ShowBoard, None),
        KeyBinding::new("cmd-3", ShowArticle, None),
        KeyBinding::new("cmd-4", ShowTable, None),
        // Bound ahead of the `tab` pair below because the menu draws the first
        // chord a command was given, and `tab` is the one it cannot draw: gpui
        // has no macOS key equivalent for it, so AppKit is handed the word
        // where the API takes one character and shows ⌃T. These are what the
        // View menu carries.
        KeyBinding::new("alt-cmd-right", NextEntry, None),
        KeyBinding::new("alt-cmd-left", PrevEntry, None),
        // What a browser binds its tabs to. Global, because the point is to
        // move between documents without taking the hand out of the editor —
        // where `tab` itself is indent.
        KeyBinding::new("ctrl-tab", NextEntry, None),
        KeyBinding::new("ctrl-shift-tab", PrevEntry, None),
        // Claimed app-wide and answered last: an editor and a field bind copy
        // on their own contexts, which gpui dispatches from the focus outward,
        // so this only runs where nothing else wanted it — which is exactly
        // where a transcript selection is the thing being copied.
        KeyBinding::new("cmd-c", CopySelection, None),
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]);
}

/// Open the workspace window. Called at launch, and again when the Dock
/// reopens an app whose window ⌘W closed.
pub fn open(settings: Settings, state: State, cx: &mut App) -> Result<WindowHandle<Cydonia>> {
    let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // No strip of its own: the traffic lights sit in the nav, so the
            // window owes no titlebar above it.
            titlebar: Some(TitlebarOptions {
                appears_transparent: true,
                traffic_light_position: Some(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y))),
                ..Default::default()
            }),
            // Glass needs a blurred window background to blur into.
            window_background: Theme::of(cx).window_background_appearance(),
            window_min_size: Some(size(px(600.), px(320.))),
            app_id: Some("cydonia".into()),
            ..Default::default()
        },
        |window, cx| {
            appearance::observe_window(window, cx).detach();
            cx.new(|cx| Cydonia::new(settings, state, window, cx))
        },
    )
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
    /// The delete waiting to be agreed to, and the name to ask about. Held
    /// with its label rather than looked up when the dialog draws: what is
    /// being asked about must not change wording under the question.
    pub(crate) confirming: Option<header::Confirming>,
    /// The board identity panel, while it is open — see [`header::BoardInfo`].
    pub(crate) info: Option<header::BoardInfo>,
    pub(crate) menu: Option<Menu>,
    /// Which of the open menu's rows is live. Held here rather than in the
    /// card, which is rebuilt every frame: the pointer moves the cursor, and
    /// a cursor made afresh each paint would light nothing.
    pub(crate) menu_cursor: Cursor,
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
                ComposerEvent::Switch(id, value) => this.switch(id, value, cx),
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
        // A re-read replaced what a pane is showing — see
        // [`Workspace::reload_project`]. The card is held by id, so it comes
        // through unless what it names is gone; the cell is still addressed by
        // where it sits, so filing it now would file it into whatever slid
        // under the index, and it is dropped. The caret follows the document,
        // which is a new editor entity.
        cx.subscribe_in(&workspace, window, |this, _, _: &Reloaded, window, cx| {
            this.drop_stale_edit(cx);
            this.cell = None;
            this.cell_field.update(cx, |field, cx| field.clear(cx));
            this.follow_article(window, cx);
            cx.notify();
        })
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
            confirming: None,
            info: None,
            menu: None,
            menu_cursor: Cursor::default(),
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

    /// Copy what the transcript has selected. Bound app-wide and reached only
    /// where nothing nearer to the focus claimed the chord.
    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.copy_selection(cx));
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

    pub(crate) fn open_settings_action(
        &mut self,
        _: &OpenSettings,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(Section::General, cx);
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

    pub(crate) fn show_chat(&mut self, _: &ShowChat, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Chat, cx);
    }

    pub(crate) fn show_board(&mut self, _: &ShowBoard, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Board, cx);
    }

    pub(crate) fn show_article(&mut self, _: &ShowArticle, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Article, cx);
    }

    pub(crate) fn show_table(&mut self, _: &ShowTable, _: &mut Window, cx: &mut Context<Self>) {
        self.show_pane(Pane::Table, cx);
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
    pub(crate) fn showing(&self, cx: &App) -> Option<Pane> {
        if self.has_pane(self.pane, cx) {
            return Some(self.pane);
        }
        [Pane::Chat, Pane::Board, Pane::Article, Pane::Table]
            .into_iter()
            .find(|&pane| self.has_pane(pane, cx))
    }

    /// Whether the active project has anything open in `pane` — what makes it
    /// a pane there is to show, as against one asked for.
    pub(crate) fn has_pane(&self, pane: Pane, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        match pane {
            Pane::Chat => workspace.active_session().is_some(),
            Pane::Board => workspace.active_board().is_some(),
            Pane::Article => workspace.active_article().is_some(),
            Pane::Table => workspace.active_table().is_some(),
        }
    }

    /// Nothing is open, so there is nowhere to send a prompt — the only thing
    /// on offer is a folder.
    pub(crate) fn no_project(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        theme
            .empty_state(
                icons::files::FOLDER,
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
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .on_action(cx.listener(Self::commit_name))
            .on_action(cx.listener(Self::commit_info))
            .on_action(cx.listener(Self::dismiss_info))
            .on_action(cx.listener(Self::dismiss_name))
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
            // Over every column and every floating control: nothing behind it
            // is answerable while it is asking.
            .children(self.confirm_delete(cx))
    }
}
