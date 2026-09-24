//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

use crate::{
    model::{
        session::ChatSession,
        settings::Settings,
        state::State,
        update,
        workspace::{Reloaded, Showing, Workspace},
    },
    view::{
        board,
        component::{
            composer::{Composer, ComposerEvent},
            menu::Menu,
            meter,
            ribbon::Ribbon,
        },
        confirm, create, info,
        leaf::{Leaf, Pane},
        menubar::CloseWindow,
        settings::{self, Section, SettingsWindow},
        sidebar::{Renaming, Row},
        table,
    },
};
use anyhow::Result;
use artifact::space::Member;
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Bounds, Context, Div, DragMoveEvent, Empty, Entity,
        FocusHandle, Focusable, Hsla, KeyBinding, PathPromptOptions, Render, SharedString,
        TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowHandle,
        WindowOptions, actions, div, point, prelude::*, px, size,
    },
    motion::{Fade, Painter},
    theme::{Material, TextStyle, Theme, Typeset, appearance},
    ui::{
        floating::Floating,
        icons,
        input::{FieldEvent, TextField},
        menu::Cursor,
        stats::Stats,
        widgets::{ButtonStyle, Buttons, Content, SplitDrag},
    },
};

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
        CommitName,
        DismissName,
        NextEntry,
        PrevEntry,
        NextPane,
        PrevPane,
        ClosePane,
        ZoomPane,
        CopySelection
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

/// The room the pill keeps above and below its content.
pub(crate) const COMPOSER_INSET: f32 = 4.;

/// Keep the send target comfortable at small text sizes, and grow with the
/// line box when the text-size setting needs more room.
pub(crate) fn composer_disc() -> f32 {
    TextStyle::Body.painted_line_height().max(24.)
}

/// How far the floating composer stands off the column's bottom edge.
pub(crate) const COMPOSER_BOTTOM: f32 = 20.;

/// How wide the composer's column runs before it stops growing, and the air it
/// keeps either side of itself inside that.
pub(crate) const COMPOSER_COLUMN: f32 = 720.;
pub(crate) const COMPOSER_MARGIN: f32 = 24.;

/// What the composer itself is at its widest — the column, less its own air.
///
/// The `/` picker is capped at this. Its rows carry a sentence each and the
/// card sizes to the longest of them, so on a wide window it opened as far as
/// the window allowed: a menu reaching past the box it came out of stops
/// reading as that box's menu.
pub(crate) fn composer_width() -> f32 {
    COMPOSER_COLUMN - 2. * COMPOSER_MARGIN
}

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

/// The band across the top of a column, and the only place its height and its
/// inset are written: the header, the sidebar's, a pane's in a space, and the
/// right panel's are all this row. A control in one stands where the same
/// control stands in the next.
///
/// The leading inset is the caller's only where the traffic lights take it —
/// see [`TOOLBAR_INSET`].
pub(crate) fn band() -> Div {
    div()
        .flex_none()
        .h(px(HEADER_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .px(px(HEADER_INSET))
}

/// Where the toolbar's own controls start: clear of the three lights AppKit
/// puts down from [`TRAFFIC_LIGHT_X`], plus the gutter that clears them and the
/// strip's own inset, so the first control stands off the lights by the same
/// measure it keeps from every other edge.
pub(crate) const TOOLBAR_INSET: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_LIGHT_X + 2. * TRAFFIC_LIGHT_SPACING + TRAFFIC_LIGHT_SIZE + 6. + HEADER_INSET
} else {
    HEADER_INSET
};

/// The chords the window keeps whatever the reader says — the commands it also
/// answers to are bound from [`crate::view::keymap`], which is where they can
/// be moved.
pub fn bindings() -> Vec<KeyBinding> {
    vec![
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
            Some(super::keymap::platform(
                "Cydonia",
                "Cydonia && !CydoniaTerminal",
            )),
        ),
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]
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
            // Glass needs a blurred window background to blur into. A window
            // that frames itself opens transparent, or its frame band is
            // filled; bezel's reapply keeps it so on each appearance switch.
            window_background: match super::chrome::decorations() {
                Some(_) => bezel::gpui::WindowBackgroundAppearance::Transparent,
                None => Theme::of(cx).window_background_appearance(),
            },
            window_min_size: Some(size(px(600.), px(320.))),
            app_id: Some("cydonia".into()),
            window_decorations: super::chrome::decorations(),
            ..Default::default()
        },
        |window, cx| {
            appearance::observe_window(window, cx).detach();
            cx.new(|cx| {
                let mut root = Cydonia::new(settings, state, window, cx);
                root.restore_panel_layout();
                cx.on_release(|root: &mut Cydonia, cx| root.save_panel_layout(cx))
                    .detach();
                // ⌘Q tears the process down without releasing the root, so a
                // release hook alone loses everything dragged in the session
                // that quit.
                cx.on_app_quit(|root: &mut Cydonia, cx| {
                    root.save_panel_layout(cx);
                    async {}
                })
                .detach();
                root
            })
        },
    )
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
/// [`Leaf`]'s. One to a window today.
pub struct Cydonia {
    pub(crate) workspace: Entity<Workspace>,
    /// The panes on screen, and which of them has the focus.
    ///
    /// One unless a space is open — see [`Leaf`]. The order is the order the
    /// arrangement lays them out, so stepping through them is stepping across
    /// the window.
    pub(crate) leaves: Vec<Leaf>,
    pub(crate) focused: usize,
    pub(crate) sidebar_open: bool,
    pub(crate) sidebar_width: f32,
    /// The window's bottom panel: its shell, and whether it is up.
    ///
    /// One to a window, like the sidebar and the right panel — every pane and
    /// every space shows this same one, and hiding it keeps its processes
    /// alive. It opens in the directory of whatever was in front at the time
    /// and stays there; `cmd-t` is how a tab somewhere else is had.
    pub(crate) terminal: Option<(bool, Entity<super::component::terminal::TerminalPanel>)>,
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
    /// How wide the right-hand panel was dragged, and `None` for one nobody
    /// has dragged — which is given a share of the window instead. See
    /// [`super::detail::panel_width`].
    pub(crate) changes_width: Option<f32>,
    /// The pending write of a width being dragged — dropped and replaced by
    /// each move, so only a drag that stopped reaches the disk. See
    /// [`Cydonia::save_panel_layout_settled`].
    pub(crate) panel_save: Option<bezel::gpui::Task<()>>,
    pub(crate) terminal_height: f32,
    pub(crate) changes: Option<Entity<super::component::panel::Panel>>,
    /// The press on a [`super::chrome::grip`], shared by every band in the
    /// window that carries one.
    pub(crate) drag: bezel::ui::titlebar::DragState,
    pub(crate) right_panels:
        std::collections::HashMap<std::path::PathBuf, Entity<super::component::panel::Panel>>,
    /// The buffer each card's orb paints into, by card id — see
    /// [`board::Marks`].
    pub(crate) card_marks: board::Marks,
    /// What each card's text parses to, by card id — see [`board::Docs`].
    pub(crate) card_docs: board::Docs,
    /// Where each board is scrolled to, by board id — see [`board::Scrolls`].
    /// On the window rather than on a pane: the same board arranged in a space
    /// and opened on its own is one board.
    pub(crate) boards: board::Scrolls,
    settings_window: Option<WindowHandle<SettingsWindow>>,
    /// The delete waiting to be agreed to, and the name to ask about. Held
    /// with its label rather than looked up when the dialog draws: what is
    /// being asked about must not change wording under the question.
    pub(crate) confirming: Option<confirm::Confirming>,
    /// The spaces whose members are folded away, by space id.
    ///
    /// Collapsed rather than expanded, so a space is open until someone folds
    /// it: an entry is listed under the space holding it and nowhere else, and
    /// a fold remembered across launches would start the window with entries
    /// hidden behind a row nobody chose to close.
    ///
    /// Runtime only, for the same reason.
    pub(crate) collapsed_spaces: std::collections::HashSet<String>,
    /// Where a pane dropped on a pane's edge would land: the pane under the
    /// pointer, and which of its edges. Written by whichever pane the pointer
    /// is inside and read by the one that draws the mark, the way a card's
    /// landing is — see [`board::Landing`].
    pub(crate) pane_landing: Option<(Member, super::arrangement::Landing)>,
    /// Which of each pane's tabs is in front, by the pane's own name — see
    /// [`Cydonia::front_of`]. Runtime only: where the panes are is the
    /// space's, and which tab you happen to be looking at is not.
    pub(crate) fronts: std::collections::HashMap<SharedString, Member>,
    /// The tabs that have been brought to the front, most recent last, across
    /// every pane. Read when a tab closes, to land on the one that was in
    /// front before it rather than on a neighbour in the strip — see
    /// [`Cydonia::close_pane`]. Runtime only, for the same reason `fronts` is.
    pub(crate) tab_history: Vec<Member>,
    /// The board identity panel, while it is open — see [`header::BoardInfo`].
    pub(crate) info: Option<info::BoardInfo>,
    /// The board that has been asked for and not yet made — see
    /// [`create::Making`].
    pub(crate) making: Option<create::Making>,
    /// Whether the press now being handled landed on the name of the board
    /// whose panel is open — read by [`Cydonia::toggle_info`] and nothing else,
    /// the way [`Cydonia::menu_pressed`] is read by `toggle_menu`.
    pub(crate) info_pressed: bool,
    pub(crate) menu: Option<Menu>,
    /// Where the open menu's card stands, when it was opened by a press with a
    /// point to it rather than from a trigger — see
    /// [`Cydonia::toggle_menu_at`].
    pub(crate) menu_point: Option<gpui::Point<gpui::Pixels>>,
    pub(crate) sidebar_hovered: Option<Menu>,
    /// Which of the open menu's rows is live. Held here rather than in the
    /// card, which is rebuilt every frame: the pointer moves the cursor, and
    /// a cursor made afresh each paint would light nothing.
    pub(crate) menu_cursor: Cursor,
    /// Whether the press now being handled landed on the open menu's own
    /// trigger — read by [`Cydonia::toggle_menu`] and nothing else.
    pub(crate) menu_pressed: bool,
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
    pub(crate) focus: FocusHandle,
}

impl Cydonia {
    /// The pane with the focus — what a command without a pane of its own acts
    /// on, and what the sidebar lights.
    ///
    /// Never empty: a window always draws at least one pane, and closing the
    /// last one is closing the space, not the pane.
    pub(crate) fn leaf(&self) -> &Leaf {
        let at = self.focused.min(self.leaves.len().saturating_sub(1));
        &self.leaves[at]
    }

    /// The entities one pane needs, with the composer wired to this window.
    ///
    /// `on` is the pane's entry, where it has one: an event from a composer in
    /// a pane that is not the focused one moves the focus there first, so what
    /// was typed is sent to the session it was typed under.
    fn pane_parts(
        on: Option<Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        Entity<Composer>,
        Entity<TextField>,
        Entity<TextField>,
        Entity<TextField>,
    ) {
        let composer = cx.new(Composer::new);
        cx.subscribe_in(
            &composer,
            window,
            move |this, _, event: &ComposerEvent, window, cx| {
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
                match event {
                    ComposerEvent::Submit(text, attachments) => {
                        this.submit(text.clone(), attachments.clone(), cx)
                    }
                    ComposerEvent::Draft(id, draft) => {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.set_draft(*id, draft.clone(), cx)
                        });
                    }
                    ComposerEvent::Cancel => this.cancel_turn(cx),
                    ComposerEvent::Reconnect => {
                        this.workspace.update(cx, |workspace, cx| {
                            if let Some(id) = workspace.active_id() {
                                workspace.select_session(id, cx);
                            }
                        });
                    }
                    ComposerEvent::Terminal => this.show_terminal(window, cx),
                    ComposerEvent::Changes => this.show_changes(window, cx),
                    ComposerEvent::Files => this.show_files(window, cx),
                    ComposerEvent::Switch(id, value) => this.switch(id, value, cx),
                }
            },
        )
        .detach();
        // The board is drawn from the root's render, so what the reader types
        // into the find field has to reach the root — the field's own `notify`
        // repaints the field alone, and the lanes would keep every card until
        // something else asked for a frame.
        let find = board::find_field(cx);
        cx.subscribe(&find, |_, _, _: &FieldEvent, cx| cx.notify())
            .detach();
        (composer, board::field(cx), table::field(cx), find)
    }

    /// Reconcile the panes on screen with the open space.
    ///
    /// A pane already on an entry is kept, so switching spaces does not throw
    /// away a composer with a draft in it. Panes are ordered as the
    /// arrangement lays them out — stepping through them steps across the
    /// window.
    /// Hand the tools what is on screen — see [`mcp::rail::shown`].
    fn publish_shown(&self, cx: &App) {
        let kind = |kind: artifact::space::Kind| match kind {
            artifact::space::Kind::Session => None,
            artifact::space::Kind::Board => Some("board"),
            artifact::space::Kind::Article => Some("article"),
            artifact::space::Kind::Table => Some("table"),
        };
        let shown = if self.leaves.iter().any(|leaf| leaf.entry.is_some()) {
            self.leaves
                .iter()
                .enumerate()
                .filter_map(|(ix, leaf)| {
                    let entry = leaf.entry.as_ref()?;
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
                    let kind = match entry.kind {
                        crate::model::state::Kind::Session => None,
                        crate::model::state::Kind::Board => Some("board"),
                        crate::model::state::Kind::Article => Some("article"),
                        crate::model::state::Kind::Table => Some("table"),
                    }?;
                    Some(mcp::rail::Shown {
                        project: project.to_path_buf(),
                        kind,
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
                // The window names an article by its `content.md`; the tools
                // name it by its id.
                if entry.kind == "article" {
                    entry.id = artifact::article::id_of(&entry.project.join(&entry.id));
                }
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
            self.leaf_mut().entry = None;
            return;
        };
        let focused = self.leaf().entry.clone();
        let mut kept: Vec<Leaf> = Vec::with_capacity(members.len());
        for entry in &members {
            match self
                .leaves
                .iter()
                .position(|leaf| leaf.entry.as_ref() == Some(entry))
            {
                Some(at) => kept.push(self.leaves.remove(at)),
                None => {
                    let (composer, card_field, cell_field, find_field) =
                        Self::pane_parts(Some(entry.clone()), window, cx);
                    let mut leaf = Leaf::new(
                        cx.focus_handle(),
                        composer,
                        card_field,
                        cell_field,
                        find_field,
                        Ribbon::new(cx),
                    );
                    leaf.entry = Some(entry.clone());
                    kept.push(leaf);
                }
            }
        }
        self.leaves = kept;
        self.focused = focused
            .and_then(|on| {
                self.leaves
                    .iter()
                    .position(|leaf| leaf.entry.as_ref() == Some(&on))
            })
            .unwrap_or(0);
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
            .position(|leaf| leaf.entry.as_ref() == Some(entry))
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
        let Some((project, showing)) = self.workspace.read(cx).showing_of(entry) else {
            // A member whose project is shut draws an empty pane, which takes
            // no caret — so the focus comes back to the window rather than
            // staying wherever it was.
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        };
        if moved {
            self.leaf_mut().pane = match showing {
                Showing::Session(_) => Pane::Chat,
                Showing::Board(_) => Pane::Board,
                Showing::Article(_) => Pane::Article,
                Showing::Table(_) => Pane::Table,
            };
            self.workspace.update(cx, |workspace, cx| {
                workspace.select_showing(project, showing, cx);
            });
        }
        self.sync_composer(cx);
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
                .session(id)
                .is_some_and(ChatSession::resumable)
                .then(|| self.composer_focus_handle(cx)),
            Showing::Article(at) => self
                .workspace
                .read(cx)
                .article_in(project, at)
                .and_then(|article| article.editor.clone())
                .map(|editor| editor.focus_handle(cx)),
            // A board or a table takes no caret of its own, but an open card
            // or cell is a field inside one: pressing into the text a second
            // time is how the caret is moved, and settling on the pane instead
            // would take the field's focus off it every time.
            Showing::Board(_) => self
                .leaf()
                .editing
                .is_some()
                .then(|| self.leaf().card_field.read(cx).focus_handle(cx)),
            Showing::Table(_) => self
                .leaf()
                .cell
                .is_some()
                .then(|| self.leaf().cell_field.read(cx).focus_handle(cx)),
        };
        // The pane's own handle, not the window's: the root's is tracked on a
        // sibling of the panes, so landing there puts the focus outside the
        // pane and the chords the pane claims stop being reached.
        let here = self.leaf().focus.clone();
        window.focus(caret.as_ref().unwrap_or(&here), cx);
        cx.notify();
    }

    /// The pane showing this entry, falling back to the focused one where the
    /// window has no space open and the caller has no entry to name.
    ///
    /// Drawing reads a pane's state through this rather than through
    /// [`Self::leaf`]: every pane drawn against the focused leaf shares one
    /// scroll, one editor and one landing between them, so moving the focus
    /// moves what the other panes are showing.
    pub(crate) fn leaf_of(&self, on: Option<&Member>) -> &Leaf {
        on.and_then(|on| {
            self.leaves
                .iter()
                .find(|leaf| leaf.entry.as_ref() == Some(on))
        })
        .unwrap_or_else(|| self.leaf())
    }

    /// The same, to write to — what a pane keeps for itself is kept on the
    /// pane, not on whichever one has the focus.
    pub(crate) fn leaf_of_mut(&mut self, on: Option<&Member>) -> &mut Leaf {
        match on.and_then(|on| {
            self.leaves
                .iter()
                .position(|leaf| leaf.entry.as_ref() == Some(on))
        }) {
            Some(at) => &mut self.leaves[at],
            None => self.leaf_mut(),
        }
    }

    pub(crate) fn leaf_mut(&mut self) -> &mut Leaf {
        let at = self.focused.min(self.leaves.len().saturating_sub(1));
        &mut self.leaves[at]
    }

    pub fn new(
        settings: Settings,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (composer, card_field, cell_field, find_field) = Self::pane_parts(None, window, cx);
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
        cx.observe_in(&workspace, window, |this, _, window, cx| {
            let previous = this.leaf().composer.read(cx).session();
            this.sync_composer(cx);
            let current = this.leaf().composer.read(cx).session();
            if current.is_some() && current != previous {
                window.focus(&this.composer_focus_handle(cx), cx);
            }
        })
        .detach();
        // The notice at the foot of the sidebar is the updater's, and the
        // updater moves on its own clock — and from the other window, where the
        // Developer switch that previews it lives.
        if let Some(updater) = update::of(cx) {
            cx.observe(&updater, |_, _, cx| cx.notify()).detach();
        }
        // A re-read replaced what a pane is showing — see
        // [`Workspace::reload_project`]. The card is held by id, so it comes
        // through unless what it names is gone; the cell is still addressed by
        // where it sits, so filing it now would file it into whatever slid
        // under the index, and it is dropped. The caret follows the document,
        // which is a new editor entity.
        cx.subscribe_in(&workspace, window, |this, _, _: &Reloaded, window, cx| {
            this.drop_stale_edit(cx);
            this.rest_ribbon(cx);
            this.leaf_mut().cell = None;
            this.leaf()
                .cell_field
                .update(cx, |field, cx| field.clear(cx));
            this.follow_article(window, cx);
            cx.notify();
        })
        .detach();

        let mut this = Self {
            meter: cx.new(Stats::new),
            meter_at: Floating::new(Painter::of(cx)),
            workspace,
            leaves: vec![Leaf::new(
                cx.focus_handle(),
                composer,
                card_field,
                cell_field,
                find_field,
                Ribbon::new(cx),
            )],
            focused: 0,
            sidebar_open: true,
            sidebar_width: SIDEBAR_WIDTH,
            terminal: None,
            changes_open: false,
            changes_for: None,
            changes_shown: Default::default(),
            changes_width: None,
            panel_save: None,
            terminal_height: 240.,
            changes: None,
            drag: Default::default(),
            right_panels: Default::default(),
            boards: Default::default(),
            card_marks: Default::default(),
            card_docs: Default::default(),
            settings_window: None,
            collapsed_spaces: Default::default(),
            pane_landing: None,
            fronts: Default::default(),
            tab_history: Vec::new(),
            confirming: None,
            info: None,
            making: None,
            info_pressed: false,
            menu: None,
            menu_point: None,
            sidebar_hovered: None,
            menu_cursor: Cursor::default(),
            menu_pressed: false,
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
        // The window comes back on the entry it was left on — the whole point
        // of [`state::Entry`], and the pane the entry is read in is half of it.
        this.land(cx);
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
        let asked = self.workspace.read(cx).preferred_agent();
        // Same as [`Self::pick_agent`]: a session with no file yet is in no
        // arrangement, so making one leaves whatever space is up.
        self.enter_member(None, window, cx);
        // Nothing to open one on: the pane says so and offers the install,
        // which is the same notice a session whose agent has gone stands
        // under. The window jumping to Settings on its own answered a question
        // it had not been asked yet.
        self.leaf_mut().asked_session = asked.is_none();
        self.show_pane(Pane::Chat, cx);
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
            .preferred_agent()
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
        if self.showing(cx) != Some(Pane::Chat) {
            return;
        }
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

    /// Step the focused pane to the next of its own tabs, wrapping at the
    /// ends. Says whether it did — a pane holding one entry has no strip of
    /// its own, and the chord means the sidebar's list instead.
    fn cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(front) = self.leaf().entry.clone() else {
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
            self.leaf_mut().pane = Pane::of(kind);
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
        let member = self.workspace.read(cx).member_of_session(id);
        if self.enter_member(member, window, cx) {
            return;
        }
        self.show_pane(Pane::Chat, cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.select_session(id, cx));
        self.sync_composer(cx);
        if self
            .workspace
            .read(cx)
            .session(id)
            .is_some_and(ChatSession::resumable)
        {
            window.focus(&self.composer_focus_handle(cx), cx);
        }
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
        if self.has_pane(self.leaf().pane, cx) {
            return Some(self.leaf().pane);
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
            // A pane with no session in it, where one was asked for and there
            // is no agent to open it on — [`Self::asked_session`]. Conditioned
            // on the agent as well as on the flag, so an install is all it
            // takes to put the pane back to what it is for.
            Pane::Chat => {
                workspace.active_session().is_some()
                    || (self.leaf().asked_session && workspace.preferred_agent().is_none())
            }
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
        self.sync_leaves(window, cx);
        self.sync_changes(cx);
        self.publish_shown(cx);
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
                match this.leaf().entry.is_some() {
                    true => this.close_focused_pane(window, cx),
                    false => window.remove_window(),
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomPane, _, cx| this.zoom_focused_pane(cx)))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::open_session_file))
            .on_action(
                cx.listener(|this, _: &OpenReview, window, cx| this.show_changes(window, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenFiles, window, cx| this.toggle_files(window, cx)))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .on_action(cx.listener(Self::commit_name))
            .on_action(cx.listener(Self::commit_info))
            .on_action(cx.listener(Self::dismiss_info))
            .on_action(cx.listener(Self::make_board))
            .on_action(cx.listener(Self::dismiss_new_board))
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
            .when(self.sidebar_open, |root| {
                root.child(self.sidebar(window, cx))
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
            .children(self.confirm_delete(cx))
            .children(self.new_board_dialog(cx));
        bezel::ui::window::frame(root, window, cx)
    }
}
