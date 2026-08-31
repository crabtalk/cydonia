//! Root view: the window's grid, the state the chrome owns, and the frame
//! the sidebar and the chat column are hung in.

use crate::{
    model::{settings::Settings, state::State, workspace::Workspace},
    view::{
        board::{self, Editing},
        component::{
            composer::{Composer, ComposerEvent},
            menu::Menu,
        },
        settings::{self, Section, SettingsWindow},
        table,
    },
};
use bezel::{
    gpui::{
        self, AnyElement, App, Axis, Context, DragMoveEvent, Empty, Entity, KeyBinding,
        PathPromptOptions, Render, Window, WindowHandle, actions, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
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
        DismissName
    ]
);

/// Claimed on the rename field so `enter` files the name and `escape` drops it.
const RENAME_CONTEXT: &str = "CydoniaSessionName";

const SIDEBAR_WIDTH: f32 = 200.;
const SIDEBAR_WIDTH_MIN: f32 = 180.;
const SIDEBAR_WIDTH_MAX: f32 = 420.;

/// Where the traffic lights sit in from the window's left edge — the
/// gallery's grid, which the sidebar container's own 16pt padding does not
/// share.
const SIDEBAR_PAD: f32 = 20.;

/// The sidebar's gutter: a row's outer margin, and the padding inside it.
pub(crate) const SIDEBAR_GUTTER: f32 = 8.;

/// How far a row under a project heading is indented. Stated as the gap it has
/// to leave rather than as a measure of its own: with the gutter added back,
/// a row's text starts on [`SIDEBAR_PAD`], where the toolbar's controls do, so
/// the sidebar has one left edge instead of one per kind of row.
pub(crate) const ROW_INDENT: f32 = SIDEBAR_PAD - SIDEBAR_GUTTER;

/// How far the content card floats in from the window's edges. The sidebar runs
/// to the floor behind it, so the frost reads as one shell under the card.
pub(crate) const SHELL_INSET: f32 = 8.;

/// macOS traffic light diameter — AppKit owns the buttons and reports their
/// frame, so nothing here can derive it. Measured on macOS 26.
const TRAFFIC_LIGHT_SIZE: f32 = 14.;

/// Where the traffic lights go, for `TitlebarOptions::traffic_light_position`:
/// the sidebar's grid across, and down by half the band the sidebar reserves for
/// them. macOS sizes the button container to `height + 2y`.
pub const TRAFFIC_LIGHT_X: f32 = SIDEBAR_PAD;
pub const TRAFFIC_LIGHT_Y: f32 = (Theme::HEADER_HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.;

/// Between the lights' centres, as AppKit lays them out. Measured on macOS 26.
const TRAFFIC_LIGHT_SPACING: f32 = 23.;

/// Where the toolbar's own controls start: clear of the three lights AppKit
/// puts down from [`TRAFFIC_LIGHT_X`], plus the sidebar's gutter. bezel's own
/// inset is for lights left where AppKit wanted them, which these are not.
pub(crate) const TOOLBAR_INSET: f32 = if cfg!(target_os = "macos") {
    TRAFFIC_LIGHT_X + 2. * TRAFFIC_LIGHT_SPACING + TRAFFIC_LIGHT_SIZE + 6.
} else {
    8.
};

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewSession, None),
        KeyBinding::new("cmd-o", OpenProject, None),
        // What macOS binds Preferences to in every other app.
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("enter", CommitName, Some(RENAME_CONTEXT)),
        KeyBinding::new("escape", DismissName, Some(RENAME_CONTEXT)),
    ]);
}

/// Which pane the content card shows. A property of the window, not of a
/// project — switching projects must not teleport you to another pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Board,
    Article,
    Table,
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
    /// The session whose name is being typed, and the field it is typed in.
    pub(crate) renaming: Option<u64>,
    pub(crate) name_field: Entity<TextField>,
}

impl Cydonia {
    pub fn new(settings: Settings, state: State, cx: &mut Context<Self>) -> Self {
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
            renaming: None,
            name_field,
        };
        this.sync_composer(cx);
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

    pub(crate) fn close_session(&mut self, id: u64, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.close_session(id, cx));
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
    /// asked for. They part when the article it points at is gone — deleted,
    /// or in a project that has none open — and the conversation stands in.
    /// The sidebar reads this, not the request: a row lit for a pane nobody can
    /// see is the second selection the eye finds.
    pub(crate) fn showing(&self, cx: &App) -> Pane {
        match self.pane {
            Pane::Article if self.workspace.read(cx).active_article().is_none() => Pane::Chat,
            Pane::Table if self.workspace.read(cx).active_table().is_none() => Pane::Chat,
            pane => pane,
        }
    }

    /// The shell strip under the content card: on the frost, not on the card.
    /// What it holds is about the pane you are in rather than anything inside
    /// it, so it sits outside the surface it switches.
    ///
    /// One button, labelled with where it goes — with two panes, a segmented
    /// track spends a permanent slot restating the one you are already
    /// looking at.
    pub(crate) fn pane_switch(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let (glyph, label, to) = match self.pane {
            Pane::Board => (icons::CHAT_ROUND_LINE, "Chat", Pane::Chat),
            Pane::Chat | Pane::Article | Pane::Table => (icons::LIST, "Board", Pane::Board),
        };
        div()
            .flex_none()
            .py(px(SHELL_INSET))
            .px(px(SHELL_INSET + 6.))
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .child(
                theme
                    .ghost("pane-switch")
                    .px(px(8.))
                    .py(px(4.))
                    .gap(px(6.))
                    .child(
                        icons::icon(glyph)
                            .size(px(13.))
                            .text_color(theme.text_faint),
                    )
                    .child(
                        div()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text_muted)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.show_pane(to, cx))),
            )
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
            .bg(theme.window_bg())
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_style(TextStyle::Body)
            .on_action(cx.listener(Self::new_session_action))
            .on_action(cx.listener(Self::commit_cell_action))
            .on_action(cx.listener(Self::dismiss_cell))
            .on_action(cx.listener(Self::open_project_action))
            .on_action(cx.listener(Self::open_settings_action))
            .on_action(cx.listener(Self::commit_name))
            .on_action(cx.listener(Self::dismiss_name))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    this.sidebar_width = f32::from(event.event.position.x)
                        .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
                    cx.notify();
                }),
            )
            .when(self.sidebar_open, |root| {
                root.child(self.sidebar(window, cx))
            })
            .child(self.detail(window, cx))
            // Rides in the gap between the sidebar and the card rather than
            // sitting in flow, so neither pane has to give up a column.
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
    }
}
