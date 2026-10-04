//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it. The window's grid lives in [`crate::view::root`];
//! this draws on it.

use crate::model::state;
#[cfg(feature = "desktop")]
use crate::model::update;
use crate::model::workspace::Showing;
use crate::view::section::Section;
use crate::{
    model::{session::ChatSession, settings::Features},
    view::{
        article::TogglePlainText,
        chrome,
        component::{
            menu::{self, Menu},
            transcript,
        },
        keymap::{self, Command},
        leaf::Pane,
        root::{self, CommitName, Cydonia, DismissName, NewSession, OpenProject},
    },
};
use artifact::board::View;
use artifact::space::Member;
use bezel::ui::scroll as scrollbars;
#[cfg(feature = "desktop")]
use bezel::ui::widgets::Content;
use bezel::{
    agent::orbs::{OrbState, engine::Frame},
    gpui::{
        self, AnyElement, App, Bounds, Context, Div, Empty, Entity, Focusable as _, FontWeight,
        Hsla, MouseButton, Pixels, Point, ScrollStrategy, SharedString, Stateful,
        UniformListDecoration, Window, div, prelude::*, px, uniform_list,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        drag,
        icons::{self, Icon},
        input::Case,
        menu::{Item, Segment},
        popover,
        surface::Surfaced as _,
        titlebar::CaptionSide,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Layout},
    },
};
use std::{
    cell::RefCell,
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

/// What the sidebar needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    id: u64,
    label: String,
    icon: Option<Icon>,
    /// The turn in flight, as the orb needs it: which of the twelve, how long
    /// it has been running, and the buffer it paints into. `None` when nothing
    /// is in flight, which is what puts the agent's own mark back.
    working: Option<Working>,
    archived: bool,
}

/// A session's orb, read off the model with the row.
struct Working {
    state: OrbState,
    since: Duration,
    frame: Rc<RefCell<Frame>>,
}

/// One line of the sidebar. An address, not content: the label behind it is
/// read when the row is built, which the list only does for the rows on
/// screen.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Row {
    /// The heading over a section of the list.
    Heading(Heading),
    /// A project or a space: a heading its entries fold under.
    Group(Group),
    /// The line the archived entries of the project at this path are folded
    /// under.
    Archive(PathBuf),
    /// An entry of the project at `project`.
    Entry { project: PathBuf, showing: Showing },
}

/// The two sections of the list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Heading {
    Projects,
    /// Drawn only while there is a space.
    Spaces,
}

impl Heading {
    /// What the heading is called, on screen and in `state.toml`.
    fn label(self) -> &'static str {
        match self {
            Self::Projects => "Projects",
            Self::Spaces => "Spaces",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Projects => "projects",
            Self::Spaces => "spaces",
        }
    }
}

/// What entries are listed under: a project holds its own, a space the ones it
/// arranges. One heading, one fold and one drag for both — see
/// [`Cydonia::group_head`].
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Group {
    /// A project, by its path.
    Project(PathBuf),
    /// A space, by its id.
    Space(String),
}

/// How a row's entry stands to what the window is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Light {
    /// In the focused pane: the one row that says where you are.
    Focused,
    /// In another pane of the open space.
    Shown,
    Off,
}

impl Light {
    /// Whether the row takes the selected wash.
    pub(crate) fn selected(self) -> bool {
        self == Self::Focused
    }

    /// The ink an entry's name is written in.
    pub(crate) fn tint(self, archived: bool, theme: &Theme) -> Hsla {
        tint(self != Self::Off, archived, theme)
    }
}

/// What an entry row stands for, and `None` for the rows that are not
/// entries.
fn showing_of(row: &Row) -> Option<&Showing> {
    match row {
        Row::Entry { showing, .. } => Some(showing),
        Row::Group(_) | Row::Archive(_) | Row::Heading(_) => None,
    }
}

/// Show `path` in the file manager, and put it on the clipboard.
fn on_disk(path: PathBuf) -> [(Item, menu::Act); 2] {
    let copied = path.to_string_lossy().into_owned();
    [
        menu::row(
            Item::action(if cfg!(target_os = "macos") {
                "Reveal in Finder"
            } else if cfg!(windows) {
                "Show in Explorer"
            } else {
                "Open in File Manager"
            })
            .with_icon(icons::files::FolderOpen),
            move |this, _, cx| this.reveal_path(path.clone(), cx),
        ),
        menu::row(
            Item::action("Copy Path").with_icon(icons::text::Copy),
            move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(copied.clone())),
        ),
    ]
}

/// The project an entry row, a project's heading or its archive line belongs
/// to.
fn project_of(row: &Row) -> Option<&Path> {
    match row {
        Row::Group(Group::Project(path)) | Row::Archive(path) => Some(path),
        Row::Entry { project, .. } => Some(project),
        Row::Group(Group::Space(_)) | Row::Heading(_) => None,
    }
}

/// Whether the kind a row names is switched on. Articles have no switch, and
/// neither do the two rows that are not entries — a project heading and the
/// line its archive folds under stand whatever is listed beneath them.
fn shown(row: &Row, features: &Features) -> bool {
    match showing_of(row) {
        Some(Showing::Session(_)) => features.sessions,
        Some(Showing::Board(_)) => features.boards,
        Some(Showing::Table(_)) => features.tables,
        Some(Showing::Article(_)) | None => true,
    }
}

/// What the sidebar's name field is attached to. One field for all of them,
/// because only one row can be being named at a time. Each entry is held by
/// what identifies it — a session, a table's key — never by an index: that
/// moves the moment a neighbour is made or dropped, and the field would follow
/// it onto whichever entry slid underneath.
///
/// No article here: its title is the first line of its own page, which is
/// where it is written — see [`crate::view::article`].
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Renaming {
    Session(u64),
    Table(String),
    /// A space, by its id — auto-named `space-1` until someone gives it a
    /// name of their own.
    Space(String),
    /// A lane, by the board it is on and its own id. The one entry here that
    /// no row in the sidebar stands for — the field is drawn in the column's
    /// own header instead, which works because only one thing is ever being
    /// named.
    Column(String, String),
}

/// What an entry's row is written in: the one on screen at full strength, one
/// put away a step back from the rest.
pub(crate) fn tint(selected: bool, archived: bool, theme: &Theme) -> Hsla {
    match (selected, archived) {
        (true, _) => theme.text,
        (false, true) => theme.text_faint,
        (false, false) => theme.text_muted,
    }
}

/// What the sidebar's list and the panes' strips carry, and what a pane takes
/// a drop of: a sidebar row, or a tab off a pane's strip.
#[derive(Clone, PartialEq)]
pub(crate) enum Dragged {
    Row(Row),
    Tab(Member),
}

/// A row's kind and the section it is listed in.
pub(crate) struct Place {
    kind: SharedString,
    section: Heading,
}

/// Whether `item` may land between the painted rows `after` and `before`.
/// An entry lands beside one of its kind. A project's or a space's heading
/// lands in front of one of its kind, or at the end of its section.
fn lands(
    places: &HashMap<Row, Place>,
    item: &Dragged,
    after: Option<&Dragged>,
    before: Option<&Dragged>,
) -> bool {
    let place = |at: Option<&Dragged>| match at {
        Some(Dragged::Row(row)) => places.get(row),
        _ => None,
    };
    let Some(own) = place(Some(item)) else {
        return false;
    };
    match item {
        Dragged::Row(Row::Group(_)) => {
            place(before).is_some_and(|place| place.kind == own.kind)
                || (place(after).is_some_and(|place| place.section == own.section)
                    && place(before).is_none_or(|place| place.section != own.section))
        }
        _ => [after, before]
            .into_iter()
            .any(|at| place(at).is_some_and(|place| place.kind == own.kind)),
    }
}

/// The rows a project's or a space's heading takes along: its group's, down
/// to the next heading.
fn carried_rows(rows: &[Row], item: &Dragged) -> Vec<Dragged> {
    let Dragged::Row(row @ Row::Group(_)) = item else {
        return Vec::new();
    };
    rows.iter()
        .skip_while(|at| *at != row)
        .skip(1)
        .take_while(|at| !matches!(at, Row::Heading(_) | Row::Group(_)))
        .map(|at| Dragged::Row(at.clone()))
        .collect()
}

/// What rides under the pointer while an item is carried to a pane.
pub(crate) fn ghost(label: SharedString, theme: &Theme) -> AnyElement {
    popover::popover_card(theme)
        .px(px(10.))
        .py(px(4.))
        .text_style(TextStyle::Callout)
        .text_color(theme.text)
        .truncate()
        .child(label)
        .into_any_element()
}

/// An entry's own name in the element tree: two rows must never share one.
pub(crate) fn key_of(entry: &Row) -> String {
    match entry {
        Row::Group(Group::Project(path)) => format!("project-{}", path.display()),
        Row::Group(Group::Space(id)) => format!("space-{id}"),
        Row::Heading(Heading::Projects) => "projects".to_owned(),
        Row::Heading(Heading::Spaces) => "spaces".to_owned(),
        Row::Archive(path) => format!("archive-{}", path.display()),
        Row::Entry { project, showing } => {
            let project = project.display();
            match showing {
                Showing::Session(id) => format!("session-{project}-{id}"),
                Showing::Board(id) => format!("board-{project}-{id}"),
                Showing::Article(id) => format!("article-{project}-{id}"),
                Showing::Table(key) => format!("table-{project}-{key}"),
            }
        }
    }
}

/// The wash a row paints, and — with the 1px either side of it that used to be
/// the column's gap — the pitch the list lays every row out at. One height for
/// headings and rows alike, because the list lays every row at one extent.
const ROW_PILL: f32 = 30.;

/// What a row puts between its mark, its name and the button at the end.
const ROW_GAP: f32 = 8.;

/// The button at the end of a row, at its full size. Named because the button
/// is laid out at no width until the pointer arrives — see
/// [`Cydonia::archive_button`]. It swaps with the row's `···`, which is a
/// [`bezel::ui::widgets::Buttons::icon_button`], so it stands at that height.
const BUTTON_SIZE: f32 = Theme::BUTTON_HEIGHT;

pub(crate) const ROW_HEIGHT: f32 = ROW_PILL + 2.;

/// How far the pinned heading's glass runs past the band it is seen in, and is
/// clipped away.
///
/// A lens bends what is behind it within [`bezel::theme::SurfaceSpec::rim`] of
/// its own edge, and lights the edge itself. On a bar one row tall that is the
/// whole of it: two bands and two hairlines, reading as a line ruled along the
/// top and the bottom. Run the glass out past the clip and only its middle —
/// the flat blur — is left in view.
const PINNED_BLEED: f32 = 20.;

/// Shared row styling keeps selection backgrounds full-width when indented.
/// One step in from the row above, for each level a row is under.
const INDENT_STEP: f32 = 14.;

pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    selected: bool,
    lifted: bool,
    indent: u8,
    theme: &Theme,
) -> Stateful<Div> {
    row_frame(id, group, indent)
        .when(lifted, |el| lift(el, theme))
        .when(!lifted && selected, |el| el.bg(theme.element_active))
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!lifted && !selected, |el| {
            el.hover(|el| el.bg(theme.element_hover))
        })
}

/// A row being carried in the list: raised the way a carried tab is, or only
/// its text would travel.
fn lift(el: Stateful<Div>, theme: &Theme) -> Stateful<Div> {
    el.bg(theme.surface_raised).cursor_grabbing()
}

/// A row's place in the column with none of its washes: what a line that is
/// never selected and takes no hover wash — a section's heading — stands in.
pub(crate) fn row_frame(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    indent: u8,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .h(px(ROW_PILL))
        .ml(px(root::SIDEBAR_GUTTER))
        .mr(px(root::SIDEBAR_GUTTER))
        .px(px(root::SIDEBAR_GUTTER))
        // Levels rather than a flag: a row a space holds is one step under
        // whatever its neighbours are at, and with the project's own indent on
        // they would otherwise land at the same offset and stop being under
        // anything.
        .when(indent > 0, |el| {
            el.pl(px(root::SIDEBAR_GUTTER + INDENT_STEP * f32::from(indent)))
        })
        .flex()
        .flex_row()
        .items_center()
        .gap(px(ROW_GAP))
        .rounded(px(Theme::control_radius()))
        .cursor_pointer()
}

/// One entry of a project, with what the list can be ordered by.
struct Ranked {
    archived: bool,
    touched: u128,
    /// Case-folded, for the comparison alone — the row draws its own name.
    name: String,
    row: Row,
}

/// A row's name. The line height is what the field pins itself to: left to
/// gpui's default the label's box is φ×13, and renaming would resize the row
/// under the name being typed.
fn row_label(name: String, tint: Hsla) -> AnyElement {
    labelled(name, tint, TextStyle::Body)
}

fn labelled(name: String, tint: Hsla, style: TextStyle) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_style(style)
        .line_height(px(18.))
        .text_color(tint)
        .child(name)
        .into_any_element()
}

/// The heading of the project whose entries are under the scroll, held at the
/// top of the list while they pass beneath it.
///
/// A decoration rather than a child of the column, because this is the one
/// place the scroll offset for the frame being drawn is known. Read off the
/// handle in `render` it would be the offset of the frame before, and the
/// heading would lag the rows it belongs to by one.
struct PinnedHead(Entity<Cydonia>);

impl UniformListDecoration for PinnedHead {
    fn compute(
        &self,
        visible: Range<usize>,
        _bounds: Bounds<Pixels>,
        scroll: Point<Pixels>,
        item_height: Pixels,
        _count: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.0.update(cx, |this, cx| {
            this.pinned_head(visible.start, scroll.y, item_height, window, cx)
        })
    }
}

impl Cydonia {
    fn sidebar_hover(&mut self, menu: Menu, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.sidebar_hovered.as_ref() == Some(&menu) {
                return;
            }
            self.sidebar_hovered = Some(menu);
        } else if self.sidebar_hovered.as_ref() == Some(&menu) {
            self.sidebar_hovered = None;
        } else {
            return;
        }
        cx.notify();
    }

    pub(crate) fn sidebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        // Taken here, where the workspace is already open, because the two
        // tooltips below are built inside closures that outlive this borrow.
        let shortcuts = &self.workspace.read(cx).settings.shortcuts;
        let settings_chord = keymap::label(Command::OpenSettings, shortcuts);
        let rows = self.rows(cx);
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(root::sidebar_bg(&theme))
            .flex()
            .flex_col()
            // The fold just past the lights, where the pane header puts it
            // with the sidebar folded: it keeps its place across the toggle.
            .child(
                root::band()
                    .pl(px(root::toolbar_inset(window, cx)))
                    .when(chrome::has(CaptionSide::Left, window, cx), |band| {
                        band.pl_0()
                    })
                    .children(chrome::caption(CaptionSide::Left, window, cx))
                    .child(self.fold_controls(cx))
                    .child(chrome::grip("sidebar-grip", &self.drag, window)),
            )
            .child(self.search_row(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar_list(rows, cx))
                    .child(
                        scrollbars::Overlay::new(
                            "sidebar-bar",
                            &self.rail.0.borrow().base_handle,
                            bezel::gpui::Axis::Vertical,
                        )
                        .visibility(
                            self.workspace
                                .read(cx)
                                .settings
                                .appearance
                                .sidebar_scrollbars
                                .into(),
                        ),
                    ),
            )
            .children(self.restart_notice(cx))
            .child(
                // Its buttons pad their glyphs by 8, which the margin makes
                // up to the column's edge.
                div()
                    .flex_none()
                    .mx(px(root::EDGE - 8.))
                    .mb(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .children(self.settings_button(settings_chord, cx)),
            )
    }

    /// The one place outside settings that says a release is in hand: a line at
    /// the foot of the sidebar, over the controls, that restarts into it.
    ///
    /// Here rather than in the header because it is news and not a control for
    /// what is on screen — and the foot of this column is already where the
    /// things that are about the app itself live. It takes a row rather than
    /// floating over one, so nothing it appears in front of is ever covered.
    ///
    /// Nothing shows here until a bundle is staged and verified, which on most
    /// days is never — see [`crate::model::update`], and the Developer section
    /// for the switch that puts it on screen without one.
    /// The gear that opens the settings window.
    fn settings_button(
        &self,
        settings_chord: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let theme = Theme::of(cx).clone();
        Some(
            theme
                .ghost("settings")
                .px(px(8.))
                .py(px(6.))
                // The mark alone, like every other control on this
                // line. What it opens is said in the tooltip, which
                // is where the two beside it say theirs.
                // The chord read off the table the keymap was
                // built from rather than typed beside the label:
                // it is the reader's to move, and a tooltip naming
                // the one it used to be is a lie nothing catches.
                .tooltip(move |window, cx| match settings_chord.clone() {
                    Some(chord) => Tooltip::with_keystroke("Settings", chord, window, cx),
                    None => Tooltip::text("Settings", window, cx),
                })
                .child(
                    icons::icon(icons::account::Settings)
                        .size(px(13.))
                        .text_color(theme.text_faint),
                )
                .on_click(cx.listener(|this, _, _, cx| this.open_settings(Section::General, cx))),
        )
    }

    #[cfg(not(feature = "desktop"))]
    fn restart_notice(&self, cx: &mut Context<Self>) -> Option<Empty> {
        let _ = cx;
        None
    }

    #[cfg(feature = "desktop")]
    fn restart_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let updater = update::of(cx)?;
        let version = updater.read(cx).ready()?;
        let theme = Theme::of(cx).clone();
        let ready = SharedString::from(format!("cydonia {version} is ready"));
        Some(
            theme
                .ghost("restart-to-update")
                .flex_none()
                .mx(px(8.))
                .mb(px(8.))
                .px(px(8.))
                .py(px(6.))
                .gap(px(6.))
                // No plate under it: `ghost` paints one on hover, and anything
                // at rest would have to be quieter than that to leave the hover
                // anything to say. What gives the line its weight is the mark,
                // which is the only accent-coloured thing in the column.
                .tooltip(move |window, cx| Tooltip::text(ready.clone(), window, cx))
                .child(
                    icons::icon(icons::development::CircleFadingArrowUp)
                        .size(px(13.))
                        .flex_none()
                        .text_color(theme.accent),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_style(TextStyle::Footnote)
                        .child("Restart to update"),
                )
                .child(theme.badge(version))
                .on_click(cx.listener(move |_, _, _, cx| {
                    updater.update(cx, |updater, cx| updater.restart(cx));
                })),
        )
    }

    /// The control that folds the sidebar away and brings it back. It belongs
    /// to whichever column runs along the window's left edge, so it changes
    /// strip across the collapse.
    pub(crate) fn fold_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let label = if self.sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        // The same glyph either way: it names the column the button acts on,
        // and the tooltip says which way it will go. A glyph that flips is a
        // second thing to read for what the label already says.
        theme
            .icon_button(
                icons::layout::PanelLeft,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), "toggle-sidebar")),
            )
            .id("toggle-sidebar")
            .flex_none()
            .tooltip(move |window, cx| Tooltip::text(label, window, cx))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
    }

    /// The fold with back and forward beside it. Every band along the
    /// window's left edge places this one element, so the three stand in the
    /// same spots whether the sidebar is open or folded.
    pub(crate) fn fold_controls(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .child(self.fold_toggle(cx))
            .child(self.history_buttons(cx))
    }

    /// The rows as one drag region over the list: an entry moves within the
    /// region of its project it is in, a project's or a space's heading among
    /// the headings of its kind, and either can be carried out onto a pane.
    fn sidebar_list(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let places = Rc::new(self.row_places(&rows, cx));
        let rows = Rc::new(rows);
        let followers = rows.clone();
        let count = rows.len();
        let list = uniform_list(
            "project-list",
            count,
            cx.processor(move |this, range: Range<usize>, window, cx| {
                let lifted: Vec<Dragged> = rows
                    .iter()
                    .map(|row| Dragged::Row(row.clone()))
                    .filter(|item| this.sidebar_sort.carries(item))
                    .flat_map(|item| {
                        let followers = carried_rows(&rows, &item);
                        std::iter::once(item).chain(followers)
                    })
                    .collect();
                range
                    .map(|ix| {
                        let row = rows[ix].clone();
                        let item = Dragged::Row(row.clone());
                        let el = this.sidebar_row(&row, lifted.contains(&item), window, cx);
                        match row {
                            Row::Heading(_) | Row::Archive(_) => {
                                this.sidebar_sort.fixed(item, el).into_any_element()
                            }
                            _ => this.sidebar_sort.handle(item, el).into_any_element(),
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.rail)
        .with_decoration(PinnedHead(cx.entity()))
        .size_full();
        self.sidebar_sort
            .region("sidebar-list", (), gpui::Axis::Vertical, list)
            .track_scroll(&self.rail.0.borrow().base_handle)
            .accepts(|item| matches!(item, Dragged::Row(_)))
            .carries(move |item| carried_rows(&followers, item))
            .lands(move |item, after, before| lands(&places, item, after, before))
            .size_full()
            .on_drop(cx.listener(|this, event: &drag::Drop<(), Dragged>, _, cx| {
                this.sidebar_moved(event, cx)
            }))
    }

    /// Where each row sits. Entries stay within the region of their project
    /// they are listed in — the pins, the rest, the archive — and a space's
    /// within that space.
    fn row_places(&self, rows: &[Row], cx: &App) -> HashMap<Row, Place> {
        let mut group: Option<&Group> = None;
        let mut section = Heading::Projects;
        rows.iter()
            .map(|row| {
                let kind = match row {
                    Row::Heading(heading) => {
                        section = *heading;
                        "heading".into()
                    }
                    Row::Archive(_) => "archive".into(),
                    Row::Group(Group::Project(_)) => "projects".into(),
                    Row::Group(Group::Space(_)) => "spaces".into(),
                    Row::Entry { project, .. } => match group {
                        Some(Group::Space(id)) => format!("space-{id}").into(),
                        _ => format!(
                            "entries-{}-{}-{}",
                            project.display(),
                            self.pinned(row, cx),
                            self.archived_of(row, cx),
                        )
                        .into(),
                    },
                };
                if let Row::Group(at) = row {
                    group = Some(at);
                }
                (row.clone(), Place { kind, section })
            })
            .collect()
    }

    /// The heading held at the top of the list, and where to hold it.
    ///
    /// Measured in the list's own space — the decoration is laid out over the
    /// whole run of rows, so `y` here is counted from the first of them rather
    /// than from the top of what is on screen.
    fn pinned_head(
        &self,
        first: usize,
        scroll: Pixels,
        item_height: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.rows(cx);
        let head = |row: &Row| matches!(row, Row::Group(_));
        let Some(at) = rows
            .get(..=first)
            .and_then(|above| above.iter().rposition(head))
        else {
            return Empty.into_any_element();
        };
        let Row::Group(group) = rows[at].clone() else {
            return Empty.into_any_element();
        };
        // The next heading pushes this one out rather than sliding under it,
        // which is what keeps two of them from ever reading as one block.
        let next = rows[at + 1..]
            .iter()
            .position(head)
            .map(|after| item_height * (at + 1 + after) - item_height);
        let rest = item_height * at;
        let y = next.map_or(-scroll, |limit| (-scroll).min(limit));
        // Above its own place there is nothing to hold: the row itself is on
        // screen, in the list, where it belongs.
        if y <= rest {
            return Empty.into_any_element();
        }
        div()
            .size_full()
            .relative()
            .child(
                // Stateful, and so an id scope of its own: the copy inside
                // carries the same ids as the row it stands for.
                //
                // The band the glass is seen in, and what clips it to one row.
                div()
                    .id("pinned-head")
                    .absolute()
                    .top(y)
                    .left_0()
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .overflow_hidden()
                    .child(self.group_head(group, true, false, window, cx)),
            )
            .into_any_element()
    }

    /// A project's or a space's heading: a press folds it. A space opens from
    /// the entries under it, and its heading is never lit — the lit row is the
    /// entry in the focused pane, see [`Self::light_of`].
    ///
    /// `pinned` is the copy [`Cydonia::pinned_head`] holds at the top of the
    /// list. It gives up the pill for the column's full width, and takes the
    /// glass the floating cluster below it is cut from — a heading with rows
    /// running under it has to be read against whatever is passing.
    fn group_head(
        &self,
        group: Group,
        pinned: bool,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let (name, folded, marks, space_id): (String, bool, (Icon, Icon), Option<String>) =
            match &group {
                Group::Project(path) => match workspace
                    .project_at(path)
                    .and_then(|ix| workspace.projects.get(ix))
                {
                    Some(project) => (
                        project.name(),
                        !project.expanded,
                        (icons::files::FolderOpen.into(), icons::files::Folder.into()),
                        None,
                    ),
                    None => return Empty.into_any_element(),
                },
                Group::Space(id) => match workspace
                    .space_ix(id)
                    .and_then(|ix| workspace.spaces.get(ix))
                {
                    Some(space) => (
                        space.label().to_owned(),
                        workspace.space_folded(&space.id),
                        (
                            icons::layout::LayoutFreeform.into(),
                            icons::layout::LayoutDashboard.into(),
                        ),
                        Some(space.id.clone()),
                    ),
                    None => return Empty.into_any_element(),
                },
            };
        let folded = folded && self.applied_query().is_none();
        let key = key_of(&Row::Group(group.clone()));
        let (menu, add) = match &group {
            Group::Project(path) => (Menu::Project(path.clone()), Some(Menu::Add(path.clone()))),
            Group::Space(_) => (Menu::Entry(Row::Group(group.clone())), None),
        };
        // The buttons stay on show while either one's menu is open. They are
        // revealed by the row's hover, and the pointer leaves the row the
        // moment it reaches the card — which took the `+` away from under a
        // menu standing open beside it.
        let held = self.menu.as_ref() == Some(&menu) || (add.is_some() && self.menu == add);
        let reveal = (!held).then_some("group-head");
        let renaming = space_id
            .as_ref()
            .is_some_and(|id| matches!(&self.renaming, Some(Renaming::Space(at)) if at == id));
        let label = match renaming {
            true => self.name_field(cx),
            false => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Callout)
                .font_weight(FontWeight::MEDIUM)
                .line_height(px(18.))
                .child(name)
                .into_any_element(),
        };
        let head = div()
            .id(SharedString::from(format!("group-{key}")))
            .group("group-head")
            .on_hover(cx.listener({
                let menu = menu.clone();
                move |this, hovered: &bool, _, cx| {
                    this.sidebar_hover(menu.clone(), *hovered, cx);
                }
            }))
            // Pinned it runs edge to edge, and past the band it shows in at
            // the top and the bottom — see [`PINNED_BLEED`]. The label keeps
            // the x the pill's own margin and padding put it at.
            .when(pinned, |el| {
                el.absolute()
                    .top(px(-PINNED_BLEED))
                    .left_0()
                    .right_0()
                    .h(px(ROW_HEIGHT + 2. * PINNED_BLEED))
                    .px(px(8. + 6.))
            })
            .when(!pinned, |el| {
                el.relative()
                    .mx(px(8.))
                    .px(px(6.))
                    .h(px(ROW_PILL))
                    .rounded(px(Theme::control_radius()))
            })
            .when(lifted, |el| lift(el, &theme))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            // On the head, not the label: a name's colour is fixed when its
            // text is laid out, and only this div is stateful enough to carry
            // the hover that far.
            .text_color(theme.text_muted)
            .hover(|el| el.text_color(theme.text))
            .child(
                theme
                    .ghost(SharedString::from(format!("group-fold-{key}")))
                    .flex_none()
                    .p(px(2.))
                    .child(
                        icons::icon(match folded {
                            false => marks.0,
                            true => marks.1,
                        })
                        .size(px(14.))
                        .text_color(theme.text_muted)
                        .group_hover("group-head", |el| el.text_color(theme.text)),
                    )
                    .on_click(cx.listener({
                        let group = group.clone();
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.fold_group(&group, cx);
                        }
                    })),
            )
            .child(label)
            // Ahead of the `+`, which is the one that gets pressed: sorting
            // and the rest are settled once and left alone.
            .child(
                self.menu_button(
                    SharedString::from(format!("group-menu-{key}")),
                    reveal,
                    icons::layout::Ellipsis,
                    menu,
                    cx,
                )
                // On the trigger, not the row: the card pins to the bottom
                // left of whatever it is mounted on, and from the row it hangs
                // off the far side of the sidebar rather than under the `···`.
                // A space's is drawn by the row around it — see
                // [`Self::sidebar_row`].
                .children(match &group {
                    Group::Project(path) => self.project_menu(path, window, cx),
                    Group::Space(_) => None,
                }),
            )
            .children(match &group {
                Group::Project(path) => Some(
                    self.menu_button(
                        SharedString::from(format!("group-add-{key}")),
                        reveal,
                        icons::math::Plus,
                        Menu::Add(path.clone()),
                        cx,
                    )
                    .children(self.add_menu(path, window, cx)),
                ),
                Group::Space(_) => None,
            })
            // A press on the copy is a press on where it came from: the list
            // goes back to the heading it is standing in for, rather than
            // folding away what you are reading. Its own mark still folds —
            // that press stops before it reaches here.
            .on_click(cx.listener({
                let group = group.clone();
                move |this, _, _, cx| match pinned {
                    true => this.scroll_to_group(&group, cx),
                    false => this.fold_group(&group, cx),
                }
            }));
        // A project's menu opens on the press rather than the click, so the
        // note has to be here too — read stale, a right press would swallow.
        // A space's opens from the row around it, as an entry's does.
        let head = match &group {
            Group::Project(path) => self
                .menu_press(head, Menu::Project(path.clone()), cx)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let menu = Menu::Project(path.clone());
                        move |this, press: &gpui::MouseDownEvent, _, cx| {
                            this.toggle_menu_at(menu.clone(), Some(press.position), cx);
                        }
                    }),
                ),
            Group::Space(_) => head,
        };
        match pinned {
            // The same token the cluster at the foot of the column mounts on,
            // so the two glasses in the sidebar move together.
            true => head
                .surface(&theme, theme.popover_surface)
                .into_any_element(),
            false => head.into_any_element(),
        }
    }

    /// Entries ordered by last user submission for sessions, last edit otherwise.
    ///
    /// One list rather than four: the kinds are told apart by their marks, and
    /// grouping by kind buries the table you are working in under every article
    /// you are not.
    pub(crate) fn entries(&self, project: usize, cx: &App) -> Vec<Row> {
        let features = &self.workspace.read(cx).settings.features;
        let mut entries = self.ranked(project, cx);
        // A project is read off disk whole whatever is switched on, so what a
        // switch hides it hides here — the entries stay in the project and in
        // memory, and turning it back on lists them again with nothing to
        // rescan.
        entries.retain(|entry| shown(&entry.row, features));
        let split = entries.iter().position(|entry| entry.archived);
        let mut entries = entries.into_iter().map(|entry| entry.row);
        let mut rows: Vec<Row> = entries.by_ref().take(split.unwrap_or(usize::MAX)).collect();
        if split.is_some()
            && let Some(open) = self.workspace.read(cx).projects.get(project)
        {
            rows.push(Row::Archive(open.path.clone()));
            if open.archive_open {
                rows.extend(entries);
            }
        }
        self.ungrouped(rows, cx)
    }

    /// Every one of a project's entries in the order the sidebar lists them,
    /// before anything is hidden or folded away.
    ///
    /// What a drag rewrites, which is why it is this list and not the one on
    /// screen: a kind switched off and an entry held by a space are both
    /// still in the project, and both keep the place they were put.
    fn ranked(&self, project: usize, cx: &App) -> Vec<Ranked> {
        let workspace = self.workspace.read(cx);
        let Some(open) = workspace.projects.get(project) else {
            return Vec::new();
        };
        // Folded for the comparison and kept that way: a sort reads it many
        // times and the case is never shown from here.
        let folded = |name: &str| name.to_lowercase();
        let row = |showing: Showing| Row::Entry {
            project: open.path.clone(),
            showing,
        };
        let sessions = open.sessions.iter().map(|chat| Ranked {
            archived: chat.closed,
            touched: chat.touched(),
            name: folded(&chat.label()),
            row: row(Showing::Session(chat.id)),
        });
        let boards = open.boards.iter().map(|board| Ranked {
            archived: board.archived,
            touched: board.touched,
            name: folded(board.label()),
            row: row(Showing::Board(board.id.clone())),
        });
        let articles = open.articles.iter().map(|article| Ranked {
            archived: article.archived,
            touched: article.touched,
            name: folded(article.label()),
            row: row(Showing::Article(article.id.clone())),
        });
        let tables = open.tables.iter().map(|table| Ranked {
            archived: table.archived,
            // The store keeps seconds; every other stamp here is milliseconds.
            touched: table.updated_at.unwrap_or(table.created_at).max(0) as u128 * 1000,
            name: folded(&table.name),
            row: row(Showing::Table(table.key.clone())),
        });
        let mut entries: Vec<Ranked> = sessions
            .chain(boards)
            .chain(articles)
            .chain(tables)
            .collect();
        let sort = workspace.sort_of(project);
        // Archived entries sink and pinned ones rise, whatever the list is
        // ordered by: what is put away is out of the way, and a pin is a place
        // somebody asked for. The sort is what happens between them.
        let head = |entry: &Ranked| {
            let pin = showing_of(&entry.row)
                .and_then(|showing| workspace.pin_rank(project, showing.clone()));
            (entry.archived, pin.is_none(), pin.unwrap_or_default())
        };
        entries.sort_by(|a, b| {
            head(a).cmp(&head(b)).then_with(|| match sort {
                state::Sort::Name => a.name.cmp(&b.name),
                state::Sort::Touched => b.touched.cmp(&a.touched),
                // The arrangement, or the entry's stamp where there is none to
                // follow — an entry made since the order was written has no
                // rank yet, and is listed above the rows that do rather than
                // under them. So a new session arrives at the top of the
                // unpinned rows without displacing a pin.
                state::Sort::Manual => {
                    let rank = |entry: &Ranked| {
                        let at = showing_of(&entry.row)
                            .and_then(|showing| workspace.rank_of(project, showing.clone()));
                        (at.is_some(), at.unwrap_or_default())
                    };
                    rank(a)
                        .cmp(&rank(b))
                        .then_with(|| b.touched.cmp(&a.touched))
                }
            })
        });
        entries
    }

    /// Drop the rows an open space holds: they are listed under it instead,
    /// and an entry is in one space at a time — see [`Workspace::arrange`] —
    /// so this is a tree and not a second copy of the list.
    fn ungrouped(&self, rows: Vec<Row>, cx: &App) -> Vec<Row> {
        let workspace = self.workspace.read(cx);
        if workspace.spaces.is_empty() {
            return rows;
        }
        rows.into_iter()
            .filter(|row| {
                self.member_of_row(row, cx)
                    .and_then(|member| workspace.space_holding(&member))
                    .is_none()
            })
            .collect()
    }

    /// The rows listed under a group while it is open: a project's own
    /// entries, or the ones a space arranges.
    ///
    /// A space's are in the order the arrangement lays the panes out, so the
    /// list reads across the window — and only what the space is the holder
    /// of. One entry is in one space at a time — see [`Workspace::arrange`] —
    /// and the same answer decides both lists, so a file written before that
    /// held lists its entry once here rather than twice, and never beside the
    /// copy [`Self::ungrouped`] took out of the project.
    fn members(&self, group: &Group, cx: &App) -> Vec<Row> {
        let workspace = self.workspace.read(cx);
        match group {
            Group::Project(path) => workspace
                .project_at(path)
                .map(|ix| self.entries(ix, cx))
                .unwrap_or_default(),
            Group::Space(id) => {
                let Some(ix) = workspace.space_ix(id) else {
                    return Vec::new();
                };
                let space = &workspace.spaces[ix];
                workspace
                    .listed_members(space)
                    .iter()
                    .filter(|member| workspace.space_holding(member) == Some(ix))
                    .filter_map(|member| self.row_of_member(member, cx))
                    .collect()
            }
        }
    }

    /// Whether a group's rows are folded away under it.
    fn folded(&self, group: &Group, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        match group {
            Group::Project(path) => workspace
                .project_at(path)
                .is_some_and(|ix| !workspace.projects[ix].expanded),
            Group::Space(id) => workspace.space_folded(id),
        }
    }

    /// Fold a group's rows away, or bring them back.
    pub(crate) fn fold_group(&mut self, group: &Group, cx: &mut Context<Self>) {
        if self.applied_query().is_some() {
            return;
        }
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| match group {
            Group::Project(path) => {
                if let Some(ix) = workspace.project_at(path) {
                    workspace.toggle_project(ix, cx);
                }
            }
            Group::Space(id) => workspace.toggle_space(id, cx),
        });
    }

    /// How far in a row is drawn: one step for a project's or an open
    /// space's rows when that is switched on.
    ///
    /// The indent is the whole of what says a row belongs to the heading above
    /// it. Nothing else is drawn on it — a rule beside it or a wash behind it
    /// is a second way of saying what the offset already says.
    pub(crate) fn indent_of(&self, row: &Row, cx: &App) -> u8 {
        match row {
            // A space is not inside a project — it can hold panes from
            // several — so its row starts at the column's edge, where the
            // project headings are.
            Row::Group(Group::Space(_)) | Row::Heading(_) => 0,
            _ => u8::from(self.workspace.read(cx).indent_project_rows),
        }
    }

    /// The row for a member a space holds — the way back from what it names
    /// to the line that stands for it. Nothing where its project is not open.
    fn row_of_member(&self, member: &Member, cx: &App) -> Option<Row> {
        let (_, showing) = self.workspace.read(cx).showing_of(member)?;
        Some(Row::Entry {
            project: member.project.clone(),
            showing,
        })
    }

    /// Every line the sidebar shows, in order. Addresses only: a project with a
    /// thousand articles costs a thousand `Row`s here and reads a title for
    /// none of them.
    pub(crate) fn rows(&self, cx: &Context<Self>) -> Vec<Row> {
        let workspace = self.workspace.read(cx);
        let projects = workspace
            .projects
            .iter()
            .map(|open| Group::Project(open.path.clone()));
        let spaces = workspace
            .spaces
            .iter()
            .map(|space| Group::Space(space.id.clone()));
        // The projects' heading whether or not any are open: it holds the way
        // to open one. After the projects, the spaces, because a space is not
        // inside one.
        let mut sections = vec![(Heading::Projects, projects.collect::<Vec<_>>())];
        if !workspace.spaces.is_empty() {
            sections.push((Heading::Spaces, spaces.collect()));
        }
        let mut rows = Vec::new();
        if let Some(matches) = self.applied_rows(cx) {
            for (heading, groups) in sections {
                let start = rows.len();
                rows.push(Row::Heading(heading));
                for group in groups {
                    let members = match &group {
                        Group::Project(path) => self.ungrouped(
                            workspace
                                .project_at(path)
                                .map(|ix| self.ranked(ix, cx))
                                .unwrap_or_default()
                                .into_iter()
                                .map(|e| e.row)
                                .collect(),
                            cx,
                        ),
                        Group::Space(_) => self.members(&group, cx),
                    };
                    let members: Vec<_> = members
                        .into_iter()
                        .filter(|row| matches.contains(row))
                        .collect();
                    if !members.is_empty() {
                        rows.push(Row::Group(group));
                        rows.extend(members);
                    }
                }
                if rows.len() == start + 1 {
                    rows.pop();
                }
            }
            return rows;
        }
        for (heading, groups) in sections {
            rows.push(Row::Heading(heading));
            if workspace.section_folded(heading.key()) {
                continue;
            }
            for group in groups {
                let open = !self.folded(&group, cx);
                let members = open.then(|| self.members(&group, cx));
                rows.push(Row::Group(group));
                rows.extend(members.into_iter().flatten());
            }
        }
        rows
    }

    /// Every open project's entries still in use, most recently touched first,
    /// with what a switch hides left out.
    pub(crate) fn recent_rows(&self, cx: &App) -> Vec<Row> {
        let workspace = self.workspace.read(cx);
        let features = &workspace.settings.features;
        let mut entries: Vec<Ranked> = (0..workspace.projects.len())
            .flat_map(|project| self.ranked(project, cx))
            .filter(|entry| !entry.archived && shown(&entry.row, features))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.touched));
        entries.into_iter().map(|entry| entry.row).collect()
    }

    /// Open what a row points at — what a keyboard step does with its landing.
    /// The pointer never comes through here: each row carries its own
    /// `on_click`, which needs no [`Row`] to know what it is.
    pub(crate) fn open_row(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        match row {
            Row::Group(Group::Project(path)) => {
                if let Some(ix) = workspace.project_at(path) {
                    self.select_project(ix, cx);
                }
            }
            Row::Archive(path) => {
                if let Some(ix) = workspace.project_at(path) {
                    self.toggle_archive(ix, cx);
                }
            }
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => self.select_session(*id, window, cx),
            Row::Entry { showing, .. } => {
                let Some((project, ix)) = self.located(row, cx) else {
                    return;
                };
                match showing {
                    Showing::Board(_) => self.open_board(project, ix, window, cx),
                    Showing::Article(_) => self.open_article(project, ix, window, cx),
                    Showing::Table(_) => self.open_table(project, ix, window, cx),
                    Showing::Session(_) => {}
                }
            }
            Row::Group(Group::Space(id)) => {
                if let Some(ix) = workspace.space_ix(id) {
                    self.open_space(ix, window, cx);
                }
            }
            // A heading over a section, and nothing to open.
            Row::Heading(_) => {}
        }
    }

    /// Where an entry row's project is listed now, and where its entry is
    /// listed among its kind in that project.
    pub(crate) fn located(&self, row: &Row, cx: &App) -> Option<(usize, usize)> {
        let Row::Entry { project, showing } = row else {
            return None;
        };
        let workspace = self.workspace.read(cx);
        let at = workspace.project_at(project)?;
        Some((at, workspace.projects[at].ix_of(showing)?))
    }

    /// Take the list back to where a group starts, heading and all.
    fn scroll_to_group(&mut self, group: &Group, cx: &mut Context<Self>) {
        let Some(at) = self
            .rows(cx)
            .iter()
            .position(|row| matches!(row, Row::Group(at) if at == group))
        else {
            return;
        };
        self.rail.scroll_to_item(at, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scroll the rail to a row, if it is not already on screen. `Nearest`
    /// rather than `Top`: a step to the neighbour below should move the list by
    /// a row, not throw the one you came from off the top of it.
    pub(crate) fn reveal(&mut self, row: &Row, cx: &Context<Self>) {
        if let Some(ix) = self.rows(cx).iter().position(|at| at == row) {
            self.rail.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    /// One line, built when the list scrolls it into view. The box around it is
    /// what holds the pitch: the row inside paints the wash, and the pixel
    /// either side of it is the gap between two.
    fn sidebar_row(
        &self,
        row: &Row,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let workspace = self.workspace.read(cx);
        let located = self.located(row, cx);
        let inner = match (row, located) {
            (Row::Group(group), _) => self.group_head(group.clone(), false, lifted, window, cx),
            (Row::Archive(path), _) => match workspace.project_at(path) {
                Some(ix) => self.archive_divider(ix, cx),
                None => Empty.into_any_element(),
            },
            (
                Row::Entry {
                    showing: Showing::Session(id),
                    ..
                },
                Some((project, _)),
            ) => match self.session_of(project, *id, cx) {
                Some(session) => self.session_row(row, session, lifted, window, cx),
                None => Empty.into_any_element(),
            },
            (
                Row::Entry {
                    showing: Showing::Board(_),
                    ..
                },
                Some((project, ix)),
            ) => {
                let name = workspace.projects[project].boards[ix].label().to_owned();
                self.board_row(row, project, ix, name, lifted, window, cx)
            }
            (
                Row::Entry {
                    showing: Showing::Article(_),
                    ..
                },
                Some((project, ix)),
            ) => {
                let title = workspace.projects[project].articles[ix].label().to_owned();
                self.article_row(row, project, ix, title, lifted, window, cx)
                    .into_any_element()
            }
            (
                Row::Entry {
                    showing: Showing::Table(_),
                    ..
                },
                Some((project, ix)),
            ) => {
                let name = workspace.projects[project].tables[ix].name.clone();
                self.table_row(row, project, ix, name, lifted, window, cx)
                    .into_any_element()
            }
            (Row::Entry { .. }, None) => Empty.into_any_element(),
            (Row::Heading(heading), _) => self.heading_row(*heading, cx),
        };
        let entry = !matches!(
            row,
            Row::Group(Group::Project(_)) | Row::Archive(_) | Row::Heading(_)
        );
        let archived = self.archived_of(row, cx);
        div()
            .id(SharedString::from(format!("sidebar-hover-{}", key_of(row))))
            .when(entry, |el| {
                el.on_hover(cx.listener({
                    let row = row.clone();
                    move |this, hovered: &bool, _, cx| {
                        this.sidebar_hover(Menu::Entry(row.clone()), *hovered, cx);
                    }
                }))
                // Every kind of row from one place, and the menu drawn here
                // rather than under whatever the row ends in: a right press
                // lands wherever the pointer is, and the trigger it opens from
                // is the row.
                .relative()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let row = row.clone();
                        move |this, press: &gpui::MouseDownEvent, _, cx| {
                            this.toggle_menu_at(Menu::Entry(row.clone()), Some(press.position), cx);
                        }
                    }),
                )
                .children(
                    (!self.pinned(row, cx))
                        .then(|| {
                            self.entry_menu(Menu::Entry(row.clone()), row, archived, window, cx)
                        })
                        .flatten(),
                )
            })
            .h(px(ROW_HEIGHT))
            .py(px(1.))
            .child(inner)
    }

    /// What the row is called, for the ghost that follows the pointer.
    pub(crate) fn label_of_row(&self, row: &Row, cx: &App) -> String {
        let workspace = self.workspace.read(cx);
        let named = || -> Option<String> {
            Some(match row {
                Row::Group(Group::Project(path)) => {
                    workspace.projects.get(workspace.project_at(path)?)?.name()
                }
                Row::Archive(_) | Row::Heading(_) => return None,
                Row::Group(Group::Space(id)) => workspace
                    .spaces
                    .get(workspace.space_ix(id)?)?
                    .label()
                    .to_owned(),
                Row::Entry { showing, .. } => {
                    let (project, ix) = self.located(row, cx)?;
                    let open = &workspace.projects[project];
                    match showing {
                        Showing::Session(_) => open.sessions[ix].label(),
                        Showing::Board(_) => open.boards[ix].label().to_owned(),
                        Showing::Article(_) => open.articles[ix].label().to_owned(),
                        Showing::Table(_) => open.tables[ix].name.clone(),
                    }
                }
            })
        };
        named().unwrap_or_default()
    }

    /// Whether a space is what the window is showing.
    pub(crate) fn arranged(&self, cx: &App) -> bool {
        self.workspace.read(cx).active_space().is_some()
    }

    /// How a row's entry stands to what the window is showing — see
    /// [`Light`]. In a space, the focused pane's entry is the lit one and the
    /// others it shows are [`Light::Shown`]; on one entry alone, that entry.
    pub(crate) fn light_of(&self, row: &Row, cx: &App) -> Light {
        let workspace = self.workspace.read(cx);
        let Some(space) = workspace.active_space() else {
            return match self.in_front(row, cx) {
                true => Light::Focused,
                false => Light::Off,
            };
        };
        let Some(member) = self.member_of_row(row, cx) else {
            return Light::Off;
        };
        if self.leaf().entry.as_ref() == Some(&member) {
            Light::Focused
        } else if space.entries().contains(&member) {
            Light::Shown
        } else {
            Light::Off
        }
    }

    /// A row let go in the list, between the painted rows `after` and
    /// `before`.
    fn sidebar_moved(&mut self, event: &drag::Drop<(), Dragged>, cx: &mut Context<Self>) {
        let Dragged::Row(carried) = &event.item else {
            return;
        };
        let rows = self.rows(cx);
        let at = |neighbour: &Option<Dragged>| match neighbour {
            Some(Dragged::Row(row)) => rows.iter().position(|at| at == row),
            _ => None,
        };
        let gap = at(&event.after)
            .map(|ix| ix + 1)
            .or_else(|| at(&event.before))
            .unwrap_or(0);
        let above = rows[..gap].iter().filter(|row| *row != carried);
        match carried {
            // Among the groups of its kind, by how many of them are above it.
            Row::Group(Group::Project(path)) => {
                let to = above
                    .filter(|row| matches!(row, Row::Group(Group::Project(_))))
                    .count();
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_project(path, to, cx));
            }
            Row::Group(Group::Space(id)) => {
                let to = above
                    .filter(|row| matches!(row, Row::Group(Group::Space(_))))
                    .count();
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_space(id, to, cx));
            }
            _ => {
                let places = self.row_places(&rows, cx);
                let kind = |row: &Row| places.get(row).map(|place| place.kind.clone());
                let own = kind(carried);
                let at = above.filter(|row| kind(row) == own).count();
                let mut region: Vec<Row> = rows
                    .iter()
                    .filter(|row| *row != carried && kind(row) == own)
                    .cloned()
                    .collect();
                region.insert(at.min(region.len()), carried.clone());
                let at = rows.iter().position(|row| row == carried).unwrap_or(0);
                let group = rows[..at].iter().rev().find_map(|row| match row {
                    Row::Group(group) => Some(group),
                    _ => None,
                });
                match group {
                    // The sidebar's order alone: the space's panes stay put.
                    Some(Group::Space(id)) => {
                        let members: Vec<Member> = region
                            .iter()
                            .filter_map(|row| self.member_of_row(row, cx))
                            .collect();
                        self.workspace.update(cx, |workspace, cx| {
                            workspace.set_space_order(id, members, cx);
                        });
                    }
                    _ => self.reorder_entries(carried, region, cx),
                }
            }
        }
    }

    /// Write the project's order down with `region` — the rows of one region
    /// of it, in their new order — taking the places those rows held.
    ///
    /// The whole list is rewritten rather than the one row that moved: an
    /// order held as gaps between the rows that did move is one every later
    /// read has to reconstruct, and the list on screen is already the answer.
    ///
    /// A drag never pins or unpins. The pins are a region of the list with
    /// their own order, and a row dragged between the two regions would be
    /// changing what it *is* rather than where it sits — that is what the
    /// button at the end of the row and the band's `···` are for.
    fn reorder_entries(&mut self, carried: &Row, region: Vec<Row>, cx: &mut Context<Self>) {
        let Some(project) =
            project_of(carried).and_then(|path| self.workspace.read(cx).project_at(path))
        else {
            return;
        };
        let rows: Vec<Row> = self
            .ranked(project, cx)
            .into_iter()
            .map(|entry| entry.row)
            .collect();
        let mut next = region.iter();
        let moved: Vec<Row> = rows
            .iter()
            .map(|row| match region.contains(row) {
                true => next.next().unwrap_or(row).clone(),
                false => row.clone(),
            })
            .collect();
        if moved == rows {
            return;
        }
        let among_pins = self.pinned(carried, cx);
        let workspace = self.workspace.read(cx);
        let entry_of = |row: &Row| workspace.entry_of(project, showing_of(row)?.clone());
        // Every entry, so that unpinning one later puts it back where it sat
        // rather than at the top.
        let order: Vec<state::Entry> = moved.iter().filter_map(entry_of).collect();
        // And the pins again, when it was one of them that moved: their own
        // order is what the region above is listed by.
        let pins: Option<Vec<state::Entry>> = among_pins.then(|| {
            moved
                .iter()
                .filter(|row| self.pinned(row, cx))
                .filter_map(entry_of)
                .collect()
        });
        self.workspace.update(cx, |workspace, cx| {
            // A drag says where a row goes, so the list goes back to being the
            // one that is arranged by hand. Under a name or a stamp the order
            // written here would be overruled on the next paint, and the row
            // would spring back to where it was let go of.
            workspace.set_sort(project, state::Sort::Manual, cx);
            workspace.set_order(project, order, cx);
            if let Some(pins) = pins {
                workspace.set_pinned(project, pins, cx);
            }
        });
    }

    /// What a space would name this row, so it can be dragged into one.
    fn member_of_row(&self, row: &Row, cx: &App) -> Option<Member> {
        let Row::Entry { project, showing } = row else {
            return None;
        };
        let workspace = self.workspace.read(cx);
        workspace.member_of(workspace.project_at(project)?, showing.clone())
    }

    /// The member a row names once it lands in a pane. A session carried with
    /// no file yet is given one here: a space names its members by file, so
    /// there is nothing to put in one until this runs. Minting it at the drop
    /// rather than at the drag keeps a gesture that went nowhere from leaving
    /// a session behind on disk.
    pub(crate) fn landed_row(&mut self, row: &Row, cx: &mut Context<Self>) -> Option<Member> {
        if let Some(member) = self.member_of_row(row, cx) {
            return Some(member);
        }
        let Row::Entry {
            project,
            showing: Showing::Session(id),
        } = row
        else {
            return None;
        };
        let id = *id;
        self.workspace.update(cx, |workspace, cx| {
            workspace.retain_session(id, cx)?;
            workspace.member_of(workspace.project_at(project)?, Showing::Session(id))
        })
    }

    /// What the sidebar needs of a session, read when its row comes on screen.
    fn session_of(&self, project: usize, id: u64, cx: &Context<Self>) -> Option<SessionRow> {
        let workspace = self.workspace.read(cx);
        let chat = workspace.projects.get(project)?.session(id)?;
        Some(SessionRow {
            id: chat.id,
            label: chat.label(),
            icon: workspace.agent_icon(&chat.entry.name),
            working: chat.streaming.then(|| Working {
                state: transcript::orb_of(chat),
                since: chat.elapsed().unwrap_or_default(),
                frame: chat.transcript.mark.clone(),
            }),
            archived: chat.closed,
        })
    }

    /// The line the archive folds under: what is put away is still listed, a
    /// step below everything still in hand.
    fn archive_divider(&self, project: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let open = self
            .workspace
            .read(cx)
            .projects
            .get(project)
            .is_some_and(|open| open.archive_open);
        row(
            ("archive", project),
            "archive-row",
            false,
            false,
            u8::from(self.workspace.read(cx).indent_project_rows),
            &theme,
        )
        .child(theme.disclosure(open).text_color(theme.text_faint))
        .child(
            div()
                .flex_none()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child("Archived"),
        )
        .child(div().flex_1().h(px(1.)).bg(theme.border))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_archive(project, cx)))
        .into_any_element()
    }

    /// A section's heading: a press folds the section, and the chevron beside
    /// the label, shown while the pointer is on the heading, says which way it
    /// stands. The projects' also holds the way to
    /// open another, shown while the pointer is on the heading.
    fn heading_row(&self, heading: Heading, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let folded =
            self.applied_query().is_none() && self.workspace.read(cx).section_folded(heading.key());
        let group: &'static str = match heading {
            Heading::Projects => "projects-heading",
            Heading::Spaces => "spaces-heading",
        };
        let open = matches!(heading, Heading::Projects).then(|| {
            let chord = keymap::label(
                Command::OpenProject,
                &self.workspace.read(cx).settings.shortcuts,
            );
            // The square every `···` and `+` in this column is, in the
            // heading's ink.
            theme
                .tinted_icon_button(icons::math::Plus, theme.text_faint)
                .id("open-project")
                .flex_none()
                .invisible()
                .group_hover(group, |el| el.visible())
                .tooltip(move |window, cx| match chord.clone() {
                    Some(chord) => Tooltip::with_keystroke("Open project", chord, window, cx),
                    None => Tooltip::text("Open project", window, cx),
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_project_action(&OpenProject, window, cx);
                }))
        });
        // A row's frame without its washes, the label ahead of its chevron.
        row_frame(group, group, 0)
            .text_color(theme.text_faint)
            .child(
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .child(heading.label()),
            )
            .child(
                div()
                    .flex_none()
                    .invisible()
                    .group_hover(group, |el| el.visible())
                    .child(theme.disclosure(!folded).text_color(theme.text_faint)),
            )
            .child(div().flex_1())
            .children(open)
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.applied_query().is_some() {
                    return;
                }
                this.workspace.update(cx, |workspace, cx| {
                    workspace.toggle_section(heading.key(), cx)
                });
            }))
            .into_any_element()
    }

    fn toggle_archive(&mut self, project: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(open) = workspace.projects.get_mut(project) {
                open.archive_open = !open.archive_open;
            }
            cx.notify();
        });
    }

    /// What the `+` starts here. Session first: it is what the sidebar is for.
    ///
    /// With more than one agent installed the session row asks which, since
    /// the one `⌘N` would pick is whoever the project last talked to — which
    /// leaves every other agent with no way in.
    fn add_menu(
        &self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !matches!(&self.menu, Some(Menu::Add(at)) if at == path) {
            return None;
        }
        let workspace = self.workspace.read(cx);
        let ix = workspace.project_at(path)?;
        let features = &workspace.settings.features;
        let (sessions, boards, tables) = (features.sessions, features.boards, features.tables);
        let agents: Vec<(String, Option<Icon>)> = workspace
            .settings
            .agents
            .iter()
            .map(|entry| (entry.name.clone(), workspace.agent_icon(&entry.name)))
            .collect();
        let mut rows = Vec::new();
        if sessions && agents.len() > 1 {
            let picks = agents
                .into_iter()
                .enumerate()
                .map(|(at, (name, icon))| {
                    let icon = icon.unwrap_or_else(|| icons::social::MessageCircle.into());
                    menu::row(
                        Item::action(name).with_icon(icon),
                        move |this, window, cx| {
                            this.select_project(ix, cx);
                            this.pick_agent(at, window, cx);
                        },
                    )
                })
                .collect();
            rows.push(menu::submenu(
                "New session",
                icons::social::MessageCirclePlus,
                picks,
            ));
        } else if sessions {
            rows.push(menu::row(
                Item::action("New session").with_icon(icons::social::MessageCirclePlus),
                move |this, window, cx| {
                    this.select_project(ix, cx);
                    this.new_session_action(&NewSession, window, cx);
                },
            ));
        }
        if boards {
            rows.push(menu::row(
                Item::action("New board").with_icon(icons::development::SquareKanban),
                move |this, window, cx| this.ask_new_board(ix, window, cx),
            ));
        }
        rows.push(menu::row(
            Item::action("New article").with_icon(icons::files::FilePlus),
            move |this, window, cx| this.new_article(ix, window, cx),
        ));
        if tables {
            rows.push(menu::row(
                Item::action("New table").with_icon(icons::files::Table2),
                move |this, window, cx| this.new_table(ix, window, cx),
            ));
        }
        let id = SharedString::from(format!("add-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, window, cx),
            None,
        ))
    }

    /// What a press on the heading opens. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_menu(
        &self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !matches!(&self.menu, Some(Menu::Project(at)) if at == path) {
            return None;
        }
        let ix = self.workspace.read(cx).project_at(path)?;
        let sort = self.workspace.read(cx).sort_of(ix);
        let by = |label: &'static str, mode: state::Sort| {
            menu::row(
                Item::action(label).checked(sort == mode),
                move |this, _, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.set_sort(ix, mode, cx));
                },
            )
        };
        let mut rows = vec![menu::submenu(
            "Sort by",
            icons::text::ArrowDownAZ,
            vec![
                by("Name", state::Sort::Name),
                by("Last modified", state::Sort::Touched),
                // Last, and named for what it is: the other two are
                // orders nobody arranged, and this is the one that is.
                by("Manual", state::Sort::Manual),
            ],
        )];
        rows.extend(on_disk(path.to_path_buf()));
        rows.push(menu::row(
            Item::action("Remove project").with_icon(icons::files::FolderMinus),
            move |this, _, cx| this.close_project(ix, cx),
        ));
        let id = SharedString::from(format!("project-menu-{ix}"));
        let card = self.menu_card(id.clone(), rows, window, cx);
        Some(match self.menu_point(&Menu::Project(path.to_path_buf())) {
            Some(point) => popover::menu_at(id, point, card, None),
            None => popover::anchored_menu_below(id, card, None),
        })
    }

    /// Show `path` in the file manager. Best effort and off the main thread:
    /// opening it is a process, and a file manager that will not come to the
    /// front is not worth blocking a frame over.
    fn reveal_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if cfg!(not(feature = "desktop")) {
            self.desktop_only("Showing it in the file manager", cx);
            return;
        }
        cx.background_executor()
            .spawn(async move {
                let _ = crate::view::component::file::external::show(&path);
            })
            .detach();
    }

    /// Where an entry row is on disk, for a backend that is the disk.
    fn place_of(&self, entry: &Row, cx: &App) -> Option<PathBuf> {
        use artifact::space::Kind;
        let (project, _) = self.located(entry, cx)?;
        let open = &self.workspace.read(cx).projects[project];
        let (kind, id) = match showing_of(entry)? {
            Showing::Article(id) => (Kind::Article, id.clone()),
            Showing::Board(id) => (Kind::Board, id.clone()),
            Showing::Session(id) => (Kind::Session, open.session(*id)?.filed()?.to_owned()),
            Showing::Table(_) => return None,
        };
        open.store().place(kind, &id)
    }

    /// One session: its mark and its name.
    fn session_row(
        &self,
        entry: &Row,
        session: SessionRow,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let id = session.id;
        let light = self.light_of(entry, cx);
        let selected = light.selected();
        let tint = light.tint(session.archived, &theme);
        // The agent's own mark, in the label's colour rather than any of its
        // own: every icon the registry publishes is a `currentColor` glyph, so
        // tinting is the only colour it will ever have. While a turn is in
        // flight the orb stands in its place — the same one the transcript
        // works under.
        let mark = if let Some(working) = session.working {
            // Wider than the slot it sits in, and left to spill: the orb is a
            // sphere where the marks around it are glyphs, and widening the
            // column for it would move every label in the sidebar to make room
            // for a row that is only sometimes working.
            transcript::orb(working.state, working.since, &working.frame, cx)
        } else {
            match session.icon {
                Some(icon) => icons::icon(icon)
                    .size(px(14.))
                    .text_color(tint)
                    .into_any_element(),
                None => Empty.into_any_element(),
            }
        };

        // The band draws the field when it is showing this entry — see
        // [`Cydonia::header_renaming`], which is what keeps one field from
        // being claimed by two places at once.
        let label = match self.renaming == Some(Renaming::Session(id))
            && self.header_renaming(cx).is_none()
        {
            true => self.name_field(cx),
            false => row_label(session.label, tint),
        };

        row(
            ("session", id),
            "session-row",
            selected,
            lifted,
            self.indent_of(entry, cx),
            &theme,
        )
        .child(
            div()
                .flex_none()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .child(mark),
        )
        .child(label)
        .child(self.archive_button(
            format!("archive-{}", key_of(entry)),
            "session-row",
            entry,
            session.archived,
            window,
            cx,
        ))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_session(id, window, cx);
        }))
        .into_any_element()
    }

    /// One board: its mark and its name.
    #[allow(clippy::too_many_arguments)]
    fn board_row(
        &self,
        entry: &Row,
        project: usize,
        ix: usize,
        name: String,
        lifted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let light = self.light_of(entry, cx);
        let selected = light.selected();
        let board = workspace
            .projects
            .get(project)
            .and_then(|open| open.boards.get(ix));
        let archived = board.is_some_and(|board| board.archived);
        let tint = light.tint(archived, &theme);
        // No inline field on a board's row, ever: a board is named by its panel
        // — see [`Self::rename_entry`].
        let label = row_label(name, tint);

        row(
            SharedString::from(key_of(entry)),
            "board-row",
            selected,
            lifted,
            self.indent_of(entry, cx),
            &theme,
        )
        .child(
            icons::icon(icons::development::SquareKanban)
                .size(px(14.))
                .flex_none()
                .text_color(tint),
        )
        .child(label)
        .child(self.archive_button(
            SharedString::from(format!("archive-{}", key_of(entry))),
            "board-row",
            entry,
            archived,
            window,
            cx,
        ))
        .on_click(cx.listener(move |this, _, window, cx| this.open_board(project, ix, window, cx)))
        .into_any_element()
    }

    /// The button every sidebar row carries in place of a menu: one press puts
    /// the entry away, or takes it back out. Rename and delete are the header's
    /// — see [`Self::entry_menu`].
    ///
    /// Shown only while the pointer is on the row, resolved from
    /// `sidebar_hovered` during render: GPUI can resolve a hover style
    /// differently in prepaint and paint.
    /// The button at the end of a row: archive, or — for a pinned row — the
    /// pin it is marked with, which opens the row's menu.
    ///
    /// A pin is the one state a row carries that nothing else on it shows, so
    /// it is drawn at rest rather than on hover. Under the pointer the same
    /// button becomes the `···`, and what a pinned row can have done to it is
    /// in the menu rather than behind a press that has to mean one of them.
    pub(crate) fn archive_button(
        &self,
        id: impl Into<SharedString>,
        group: &'static str,
        entry: &Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let id = id.into();
        let pinned = self.pinned(entry, cx);
        if pinned {
            let at = Menu::Entry(entry.clone());
            // The glyph alone turns over; the button is the same button either
            // way, so a press during the frame the hover is still travelling
            // opens the menu rather than falling through to the row.
            let open = self.menu.as_ref() == Some(&at);
            let mark = match open || self.sidebar_hovered.as_ref() == Some(&at) {
                true => icons::layout::Ellipsis,
                false => icons::navigation::Pin,
            };
            return self
                .menu_button(id, None, mark, at.clone(), cx)
                // On the trigger, so the card hangs under the `···` rather
                // than off the left edge of the row it is mounted on. A row
                // with no button of its own draws it from the wrapper, where
                // a right press is all there is to anchor to.
                .children(self.entry_menu(at, entry, archived, window, cx));
        }
        let held = matches!(&self.menu, Some(Menu::Entry(at)) if at == entry);
        let entry = entry.clone();
        let mark = match archived {
            true => icons::files::ArchiveRestore,
            false => icons::files::Archive,
        };
        theme
            .icon_button(
                mark,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), id.clone())),
            )
            .id(id)
            .flex_none()
            // Out of sight but laid out, and revealed off the row's own hover
            // group rather than [`Cydonia::sidebar_hovered`].
            //
            // A group resolves inside the frame the pointer arrives on; a
            // field read at render is a frame behind, because the hover has to
            // go through the model and come back as a repaint. An element out
            // of sight registers no mouse handler either way, so that frame is
            // one in which the button cannot be pressed — and a press landing
            // in it goes to the row instead and reads as a click that did
            // nothing.
            // Held open while its row's menu is: the pointer leaves the row
            // the moment it reaches the card, and a button that collapsed
            // then would take the row's shape with it.
            .when(!held, |el| {
                el.invisible()
                    .w(px(0.))
                    .ml(px(-ROW_GAP))
                    .group_hover(group, |el| el.visible().w(px(BUTTON_SIZE)).ml(px(0.)))
            })
            // And taking no width until then. Laid out at its full size the
            // button is a column down the whole list, holding space nothing is
            // in and truncating every name by what an archive glyph would take
            // — which is only ever wanted under the pointer. The row's `gap`
            // still falls either side of a child with no width, so the margin
            // that cancels it comes back with the width.
            .overflow_hidden()
            .tooltip(move |window, cx| {
                Tooltip::text(
                    match archived {
                        true => "Unarchive",
                        false => "Archive",
                    },
                    window,
                    cx,
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.archive_entry(&entry, !archived, window, cx);
            }))
    }

    /// Whether a row's entry is put away. `false` for the rows that are not
    /// entries, and for a space — archiving one drops the arrangement rather
    /// than filing it.
    fn archived_of(&self, row: &Row, cx: &App) -> bool {
        let Row::Entry { showing, .. } = row else {
            return false;
        };
        let Some((project, ix)) = self.located(row, cx) else {
            return false;
        };
        let open = &self.workspace.read(cx).projects[project];
        match showing {
            Showing::Session(_) => open.sessions[ix].closed,
            Showing::Board(_) => open.boards[ix].archived,
            Showing::Article(_) => open.articles[ix].archived,
            Showing::Table(_) => open.tables[ix].archived,
        }
    }

    /// Whether an entry is held at the top of its project's list.
    pub(crate) fn pinned(&self, entry: &Row, cx: &App) -> bool {
        let Row::Entry { project, showing } = entry else {
            return false;
        };
        let workspace = self.workspace.read(cx);
        workspace
            .project_at(project)
            .is_some_and(|project| workspace.is_pinned(project, showing.clone()))
    }

    /// Pin an entry to the top of its project's list, or let it back down.
    pub(crate) fn pin_entry(&mut self, entry: &Row, on: bool, cx: &mut Context<Self>) {
        let Row::Entry { project, showing } = entry else {
            return;
        };
        self.workspace.update(cx, |workspace, cx| {
            if let Some(project) = workspace.project_at(project) {
                workspace.pin(project, showing.clone(), on, cx);
            }
        });
    }

    /// Everything an entry can have done to it: the `···` in the band, and the
    /// menu a sidebar row opens on a right press.
    ///
    /// One builder for both, so a command reachable in the band is reachable
    /// on the row. What a row still carries of its own is the press in
    /// passing — archive for an unpinned row, the pin for a pinned one, which
    /// opens this rather than acting. See [`Self::archive_button`].
    pub(crate) fn entry_menu(
        &self,
        at: Menu,
        entry: &Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&at) {
            return None;
        }
        let put = match archived {
            true => Item::action("Unarchive").with_icon(icons::files::ArchiveRestore),
            false => Item::action("Archive").with_icon(icons::files::Archive),
        };
        // `../desktop`'s rule for what a `···` may carry: only commands with no
        // affordance on the object. An article's title is the head of its own
        // page and a board's name in the band opens its identity panel, so
        // neither is offered a second route here. Everywhere else the name is
        // display-only and this is the way.
        let named = !matches!(
            showing_of(entry),
            Some(Showing::Article(_) | Showing::Board(_))
        );
        let mut rows = vec![menu::row(put, {
            let entry = entry.clone();
            move |this, window, cx| this.archive_entry(&entry, !archived, window, cx)
        })];
        // Above archive, and only for an entry still in hand: what is put away
        // is not held at the top of anything.
        if !archived && showing_of(entry).is_some() {
            let pinned = self.pinned(entry, cx);
            let pin = match pinned {
                true => Item::action("Unpin").with_icon(icons::navigation::PinOff),
                false => Item::action("Pin to top").with_icon(icons::navigation::Pin),
            };
            let entry = entry.clone();
            rows.insert(
                0,
                menu::row(pin, move |this, _, cx| this.pin_entry(&entry, !pinned, cx)),
            );
        }
        if named {
            rows.insert(
                0,
                menu::row(Item::action("Rename").with_icon(icons::text::SquarePen), {
                    let entry = entry.clone();
                    move |this, window, cx| this.rename_entry(&entry, window, cx)
                }),
            );
        }
        // A page's measure and how it is being read: the open page's, since
        // [`Self::set_full_width`] and [`Self::plain_text`] are about the one
        // the window is showing. A row's menu names an entry that may not be
        // it, so these are the band's alone — on the wrong row they would act
        // on whatever else was open.
        // A tab's menu has them only while its tab is the one focused.
        let page = match &at {
            Menu::Entry(_) => false,
            Menu::Tab(tab) => self.leaf().entry.as_ref() == Some(tab),
            _ => true,
        };
        if matches!(showing_of(entry), Some(Showing::Article(_))) && page {
            let plain_chord = keymap::label(
                Command::PlainText,
                &self.workspace.read(cx).settings.shortcuts,
            )
            .unwrap_or_default();
            // The page the focused pane is on, which is what these rows act on
            // — see [`Cydonia::pane_doc`].
            let held = self.pane_doc(cx).and_then(|article| article.full_width);
            let wide = held.unwrap_or(self.workspace.read(cx).wide_pages);
            // Only where there is none. A page that has one is changed from
            // the picture itself, which is on screen and has nowhere else it
            // could mean — see `article::cover_controls`.
            if self
                .pane_doc(cx)
                .is_some_and(|article| article.cover.is_none())
            {
                rows.insert(
                    0,
                    menu::row(
                        Item::action("Add cover").with_icon(icons::files::ImagePlus),
                        move |this, _, cx| this.shuffle_cover(cx),
                    ),
                );
            }
            // Only for a page carrying a measure of its own. On every other
            // page it is already what is happening, and a row that undoes
            // nothing is a row nobody can read the point of.
            if held.is_some() {
                rows.insert(
                    0,
                    menu::row(
                        Item::action("Use default width").with_icon(icons::layout::Columns2),
                        move |this, _, cx| this.set_full_width(None, cx),
                    ),
                );
            }
            rows.insert(
                0,
                menu::row(
                    Item::action("Full width")
                        .with_icon(icons::layout::UnfoldHorizontal)
                        .checked(wide),
                    move |this, _, cx| this.set_full_width(Some(!wide), cx),
                ),
            );
            // The markdown itself, for the times the document is in the way of
            // it. Above the width, which is about the page rather than what is
            // being edited on it.
            rows.insert(
                0,
                menu::row(
                    Item::action("Plain text")
                        .with_icon(icons::text::Code)
                        .with_keystroke(plain_chord)
                        .checked(self.plain_text(cx).unwrap_or_default()),
                    move |this, window, cx| this.toggle_plain_text(&TogglePlainText, window, cx),
                ),
            );
        }
        // Into any other open project, the folder and its pictures with it.
        if let Some(Showing::Article(_)) = showing_of(entry)
            && let Some((project, ix)) = self.located(entry, cx)
        {
            let targets: Vec<(Item, menu::Act)> = self
                .workspace
                .read(cx)
                .projects
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != project)
                .map(|(to, open)| {
                    menu::row(
                        Item::action(open.name()).with_icon(icons::files::Folder),
                        move |this, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.move_article(project, ix, to, cx)
                            });
                        },
                    )
                })
                .collect();
            if !targets.is_empty() {
                rows.push(menu::submenu(
                    "Move to",
                    icons::arrows::ArrowRightLeft,
                    targets,
                ));
            }
        }
        if let Some(path) = self.place_of(entry, cx) {
            rows.extend(on_disk(path));
        }
        rows.push(menu::row(
            Item::action("Delete").with_icon(icons::files::Trash),
            {
                let entry = entry.clone();
                move |this, _, cx| this.ask_delete(&entry, cx)
            },
        ));
        if let Some(Showing::Board(_)) = showing_of(entry)
            && !matches!(at, Menu::Entry(_))
            && let Some((project, ix)) = self.located(entry, cx)
            && let Some((id, view)) = self
                .workspace
                .read(cx)
                .board_in(project, ix)
                .map(|board| (board.id.clone(), board.view))
        {
            let views = [View::Lanes, View::List];
            let item = Item::segmented(
                [
                    Segment::new(icons::development::SquareKanban, "Lanes"),
                    Segment::new(icons::layout::LayoutList, "List"),
                ],
                views.iter().position(|at| *at == view).unwrap_or_default(),
            );
            let act: menu::Act = Box::new(move |this, path, _, cx| {
                if let Some(view) = path.first().and_then(|at| views.get(*at)) {
                    this.set_board_view(&id, *view, cx);
                }
            });
            rows.insert(0, (item, act));
        }
        let id = SharedString::from("header-menu-card");
        // A right press carries a point, and the card stands at it. From a
        // button — the `···`, the pin — there is none, and the card drops
        // right-aligned to the trigger, whose affordance is at the row's end.
        Some(match self.menu_point(&at) {
            Some(point) => popover::menu_at(
                id.clone(),
                point,
                self.menu_card(id, rows, window, cx),
                None,
            ),
            None => popover::anchored_menu_below_end(
                id.clone(),
                self.menu_card(id, rows, window, cx),
                None,
            ),
        })
    }

    /// Drop an entry, file and all. Deleting the session on screen lands on
    /// the first remaining entry in the sidebar's displayed order.
    pub(crate) fn delete_entry(
        &mut self,
        entry: &Row,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let located = self.located(entry, cx);
        let landing_project = match (entry, located) {
            (
                Row::Entry {
                    showing: Showing::Session(id),
                    ..
                },
                Some((project, _)),
            ) if self.showing(cx) == Some(Pane::Chat)
                && self.workspace.read(cx).active == Some(project)
                && self.workspace.read(cx).active_id() == Some(*id) =>
            {
                Some(project)
            }
            _ => None,
        };
        let member = self.member_of_row(entry, cx);
        let arranged = self.put_away_arranged(member.as_ref(), window, cx);
        let landing_project = landing_project.filter(|_| !arranged);
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| {
            if let Some(member) = &member {
                workspace.drop_from_spaces(member, cx);
            }
            match (entry, located) {
                (
                    Row::Entry {
                        showing: Showing::Session(id),
                        ..
                    },
                    _,
                ) => workspace.close_session(*id, cx),
                (Row::Entry { showing, .. }, Some((project, ix))) => match showing {
                    Showing::Board(_) => workspace.delete_board(project, ix, cx),
                    Showing::Article(_) => workspace.delete_article(project, ix, cx),
                    Showing::Table(_) => workspace.delete_table(project, ix, cx),
                    Showing::Session(_) => {}
                },
                (Row::Group(Group::Space(id)), _) => workspace.delete_space_id(id, cx),
                (Row::Entry { .. }, None)
                | (Row::Group(Group::Project(_)) | Row::Archive(_) | Row::Heading(_), _) => {}
            }
        });
        if let Some(project) = landing_project
            && let Some(landing) = self
                .entries(project, cx)
                .into_iter()
                .find(|row| !matches!(row, Row::Archive(_)))
        {
            self.open_row(&landing, window, cx);
            self.reveal(&landing, cx);
        }
        cx.notify();
    }

    /// Put the name field on an entry's row, for the kinds named that way.
    ///
    /// A board is named by two things at once, so it opens its identity panel
    /// instead — which lives under the band, so the board is brought to the
    /// front first. One way to name a board, wherever you asked from.
    fn rename_entry(&mut self, entry: &Row, window: &mut Window, cx: &mut Context<Self>) {
        let what = match entry {
            Row::Entry {
                showing: Showing::Board(id),
                ..
            } => {
                if let Some((project, ix)) = self.located(entry, cx) {
                    self.open_board(project, ix, window, cx);
                    self.open_info(id, window, cx);
                }
                return;
            }
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => Some(Renaming::Session(*id)),
            Row::Entry {
                showing: Showing::Table(key),
                ..
            } => Some(Renaming::Table(key.clone())),
            Row::Group(Group::Space(id)) => Some(Renaming::Space(id.clone())),
            // An article is named in its own page, and the two that are not
            // entries have no name to take.
            Row::Entry {
                showing: Showing::Article(_),
                ..
            }
            | Row::Group(Group::Project(_))
            | Row::Archive(_)
            | Row::Heading(_) => None,
        };
        if let Some(what) = what {
            self.start_rename(what, window, cx);
        }
    }

    /// Put an entry away, or bring it back. Where the flag lives is each
    /// kind's own business — a board's file, an article's properties, a row in
    /// the store — and the sidebar asks for it the same way.
    fn archive_entry(
        &mut self,
        entry: &Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read before the flag moves: once the entry is away it is no longer
        // what any pane is on.
        let landing = (archived && self.in_front(entry, cx))
            .then(|| self.located(entry, cx).map(|(project, _)| project))
            .flatten();
        // A space takes its members with it. Membership is exclusive — an
        // entry is in one space at a time — so the arrangement owns what it
        // holds, and putting it away that holds nothing would be putting away
        // an empty row.
        //
        // The arrangement itself is dropped rather than put away: a space
        // names entries and holds none, so there is nothing in one to come
        // back to, and it is remade by dragging one entry onto another.
        if let Row::Group(Group::Space(id)) = entry {
            let workspace = self.workspace.read(cx);
            let Some(space) = workspace.space_ix(id).map(|ix| &workspace.spaces[ix]) else {
                return;
            };
            let members: Vec<Row> = space
                .entries()
                .iter()
                .filter_map(|member| self.row_of_member(member, cx))
                .collect();
            for member in &members {
                self.archive_entry(member, archived, window, cx);
            }
            self.workspace
                .update(cx, |workspace, cx| workspace.delete_space_id(id, cx));
            return;
        }
        let Row::Entry { project, showing } = entry else {
            return;
        };
        // Putting an entry away takes it out of the two places that hold it
        // up: the pins at the top of the list, and whatever space arranges
        // it. Both are about an entry in hand, and this one no longer is.
        if archived {
            let member = self.member_of_row(entry, cx);
            self.put_away_arranged(member.as_ref(), window, cx);
            self.workspace.update(cx, |workspace, cx| {
                if let Some(project) = workspace.project_at(project) {
                    workspace.unpin_entry(project, showing.clone());
                }
                if let Some(member) = member {
                    workspace.drop_from_spaces(&member, cx);
                }
            });
        }
        let article = match (showing, self.located(entry, cx)) {
            (Showing::Article(_), Some((project, ix))) => Some(
                self.workspace.read(cx).projects[project].articles[ix]
                    .path
                    .clone(),
            ),
            _ => None,
        };
        self.workspace.update(cx, |workspace, cx| match showing {
            Showing::Session(id) => workspace.archive_session(*id, archived, cx),
            Showing::Board(id) => workspace.archive_board(id, archived, cx),
            Showing::Article(_) => {
                if let Some(path) = article {
                    workspace.archive_article(&path, archived, cx);
                }
            }
            Showing::Table(key) => workspace.archive_table(key, archived, cx),
        });
        if let Some(project) = landing {
            self.open_top_entry(project, window, cx);
        }
    }

    /// Close the member's pane when the open space holds it — see
    /// [`Cydonia::put_away_pane`]. Answers whether it did.
    fn put_away_arranged(
        &mut self,
        member: Option<&Member>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(member) = member.filter(|member| {
            self.arrangement(cx)
                .is_some_and(|space| space.contains(member))
        }) else {
            return false;
        };
        self.put_away_pane(&member.clone(), window, cx);
        true
    }

    /// Whether the pane in front is on this entry.
    fn in_front(&self, entry: &Row, cx: &App) -> bool {
        let Row::Entry { showing, .. } = entry else {
            return false;
        };
        let Some((project, ix)) = self.located(entry, cx) else {
            return false;
        };
        let workspace = self.workspace.read(cx);
        if workspace.active != Some(project) || self.arranged(cx) {
            return false;
        }
        let open = &workspace.projects[project];
        let pane = self.showing(cx);
        match showing {
            Showing::Session(id) => pane == Some(Pane::Chat) && open.active == Some(*id),
            Showing::Board(_) => pane == Some(Pane::Board) && open.board == Some(ix),
            Showing::Article(_) => pane == Some(Pane::Article) && open.article == Some(ix),
            Showing::Table(_) => pane == Some(Pane::Table) && open.table == Some(ix),
        }
    }

    /// Put the pane on the first entry the project still lists. What the pane
    /// falls back to when the entry it was on is put away — [`Self::entries`]
    /// sorts the archived below the divider, so the first row is one still in
    /// hand or the divider itself.
    fn open_top_entry(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(top) = self
            .entries(project, cx)
            .into_iter()
            .find(|row| showing_of(row).is_some())
        else {
            return;
        };
        self.open_row(&top, window, cx);
    }

    /// The field, in the row's place. It carries its own press: `TextField`
    /// does not focus itself, and a press that reached the row would open what
    /// is being named out from under the name.
    pub(crate) fn name_field(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.focus(&this.name_field.read(cx).focus_handle(cx), cx);
                }),
            )
            // Pressing anywhere else is finishing, not abandoning — the name
            // typed is the name meant. `escape` is what discards.
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.commit_name(&CommitName, window, cx);
            }))
            .child(self.name_field.clone())
            .into_any_element()
    }

    pub(crate) fn start_rename(
        &mut self,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.read(cx);
        let label = match &what {
            Renaming::Session(id) => workspace
                .session(*id)
                .map(ChatSession::label)
                .unwrap_or_default(),
            Renaming::Table(key) => workspace
                .projects
                .iter()
                .flat_map(|open| open.tables.iter())
                .find(|table| table.key == *key)
                .map(|table| table.name.clone())
                .unwrap_or_default(),
            Renaming::Column(board, id) => workspace
                .board_at(board)
                .and_then(|board| board.column(id))
                .map(|column| column.name.clone())
                .unwrap_or_default(),
            Renaming::Space(id) => workspace
                .spaces
                .iter()
                .find(|space| space.id == *id)
                .map(|space| space.name.clone())
                .unwrap_or_default(),
        };
        // A lane is named in one case — see [`artifact::board::column::heading`]
        // — and the field is put in it before the name lands, so what is typed
        // and what is stored are the same string.
        let case = match &what {
            Renaming::Column(..) => Case::Upper,
            _ => Case::Mixed,
        };
        self.name_field.update(cx, |field, cx| {
            field.set_case(case);
            field.set_content(label, cx);
        });
        // See [`Cydonia::open_info`] — the other way round.
        self.info = None;
        self.renaming = Some(what);
        window.focus(&self.name_field.read(cx).focus_handle(cx), cx);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, _: &CommitName, _: &mut Window, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        let name = self.name_field.read(cx).content().to_string();
        self.workspace.update(cx, |workspace, cx| match what {
            Renaming::Session(id) => workspace.rename_session(id, name, cx),
            Renaming::Table(key) => workspace.rename_table(&key, name, cx),
            Renaming::Column(board, id) => workspace.rename_column(&board, &id, name, cx),
            Renaming::Space(id) => workspace.rename_space(&id, name, cx),
        });
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, _: &DismissName, _: &mut Window, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
