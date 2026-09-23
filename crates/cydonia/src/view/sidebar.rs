//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it. The window's grid lives in [`crate::view::root`];
//! this draws on it.

use crate::model::state;
use crate::model::workspace::Showing;
use crate::{
    model::{session::ChatSession, settings::Features, update},
    view::{
        article::TogglePlainText,
        component::{
            menu::{self, Menu},
            transcript,
        },
        keymap::{self, Command},
        leaf::Pane,
        root::{self, CommitName, Cydonia, DismissName, NewSession, OpenProject},
        settings::Section,
    },
};
use artifact::space::Member;
use bezel::ui::scroll as scrollbars;
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
        icons::{self, Icon},
        input::Case,
        menu::Item,
        popover,
        surface::Surfaced as _,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Layout},
    },
};
use std::{cell::RefCell, ops::Range, rc::Rc, time::Duration};

/// What the sidebar needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    project: usize,
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
/// read when the row is built, which [`uniform_list`] only does for the rows on
/// screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Project(usize),
    /// The line the archived entries are folded under.
    Archive(usize),
    /// The line the spaces are listed under. Drawn only while there is one.
    Spaces,
    Session {
        project: usize,
        id: u64,
    },
    Board {
        project: usize,
        ix: usize,
    },
    Space(usize),
    Article {
        project: usize,
        ix: usize,
    },
    Table {
        project: usize,
        ix: usize,
    },
}

/// Which of a project's four an entry row stands for, and `None` for the two
/// rows that are not entries.
fn showing_of(row: Row) -> Option<Showing> {
    Some(match row {
        // A space arranges entries; it is not one a pane can be put on.
        Row::Project(_) | Row::Archive(_) | Row::Space(_) | Row::Spaces => return None,
        Row::Session { id, .. } => Showing::Session(id),
        Row::Board { ix, .. } => Showing::Board(ix),
        Row::Article { ix, .. } => Showing::Article(ix),
        Row::Table { ix, .. } => Showing::Table(ix),
    })
}

/// The project an entry row belongs to.
fn project_of(row: Row) -> Option<usize> {
    match row {
        Row::Project(ix) | Row::Archive(ix) => Some(ix),
        Row::Session { project, .. }
        | Row::Board { project, .. }
        | Row::Article { project, .. }
        | Row::Table { project, .. } => Some(project),
        Row::Space(_) | Row::Spaces => None,
    }
}

/// Whether the kind a row names is switched on. Articles have no switch, and
/// neither do the two rows that are not entries — a project heading and the
/// line its archive folds under stand whatever is listed beneath them.
fn shown(row: Row, features: &Features) -> bool {
    match row {
        Row::Session { .. } => features.sessions,
        Row::Board { .. } => features.boards,
        Row::Table { .. } => features.tables,
        Row::Project(_) | Row::Archive(_) | Row::Article { .. } | Row::Space(_) | Row::Spaces => {
            true
        }
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

/// A project on its way to another place in the list. The index is safe to
/// carry: nothing reorders the list while a drag is in flight.
#[derive(Clone)]
pub(crate) struct ProjectDrag(usize);

/// A space on its way to another place in the list, carried the way a project
/// is. Its own drag rather than [`EntryDrag`]: a space is not something a
/// pane can be put on, and the two lists never mix.
#[derive(Clone)]
pub(crate) struct SpaceDrag(usize);

/// What rides under the cursor while an entry is being carried. Shared with
/// the panes, so a tab dragged out of a strip looks like the same gesture the
/// sidebar makes — see [`crate::view::arrangement`].
pub(crate) struct Carried(pub SharedString);

/// An entry carried out of the sidebar, named the way a space names its
/// members — or, for a session with no file yet, named by the session it is.
#[derive(Clone, Debug)]
pub enum EntryDrag {
    Member(artifact::space::Member),
    /// A session that has had no turn. A space names its members by file and
    /// this one has none, so it is carried by the id it holds in this process
    /// and the file is minted where it lands.
    Session {
        project: usize,
        id: u64,
    },
}

impl Render for Carried {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        popover::popover_card(&theme)
            .px(px(10.))
            .py(px(4.))
            .text_style(TextStyle::Callout)
            .text_color(theme.text)
            .child(self.0.clone())
    }
}

/// An entry's own name in the element tree: two rows must never share one.
fn key_of(entry: Row) -> String {
    match entry {
        Row::Project(ix) => format!("project-{ix}"),
        Row::Space(ix) => format!("space-{ix}"),
        Row::Spaces => "spaces".to_owned(),
        Row::Archive(ix) => format!("archive-{ix}"),
        Row::Session { project, id } => format!("session-{project}-{id}"),
        Row::Board { project, ix } => format!("board-{project}-{ix}"),
        Row::Article { project, ix } => format!("article-{project}-{ix}"),
        Row::Table { project, ix } => format!("table-{project}-{ix}"),
    }
}

/// The wash a row paints, and — with the 1px either side of it that used to be
/// the column's gap — the pitch the list lays every row out at. One height for
/// headings and rows alike, because [`uniform_list`] measures a single row and
/// gives every other one the same.
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
    indent: u8,
    theme: &Theme,
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
        .when(selected, |el| el.bg(theme.element_active))
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!selected, |el| el.hover(|el| el.bg(theme.element_hover)))
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

/// The same, in the type a heading is written in — see
/// [`Cydonia::space_row`].
///
/// One shape behind both: `flex_1` is what fills the row and so what holds a
/// trailing button out at the edge, and `truncate` is what ellipsizes rather
/// than clipping a word in half. A label built without them looks right until
/// the row has something after it or the name is long.
fn row_heading(name: String, tint: Hsla) -> AnyElement {
    labelled(name, tint, TextStyle::Callout)
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
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.0.update(cx, |this, cx| {
            this.pinned_head(visible.start, scroll.y, item_height, cx)
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

    pub(crate) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        // Taken here, where the workspace is already open, because the two
        // tooltips below are built inside closures that outlive this borrow.
        let shortcuts = &self.workspace.read(cx).settings.shortcuts;
        let settings_chord = keymap::label(Command::OpenSettings, shortcuts);
        let open_chord = keymap::label(Command::OpenProject, shortcuts);
        let rows = self.rows(cx);
        let count = rows.len();
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(root::sidebar_bg(&theme))
            .flex()
            .flex_col()
            // The fold out at the trailing edge: the lights float in the
            // leading half of the strip, which is what leaves nothing there to
            // pad them clear of.
            .child(
                root::band()
                    .justify_end()
                    .child(self.fold_toggle(cx)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "project-list",
                            count,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                range.map(|ix| this.sidebar_row(rows[ix], cx)).collect()
                            }),
                        )
                        .track_scroll(&self.rail)
                        .with_decoration(PinnedHead(cx.entity()))
                        .size_full(),
                    )
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
                div()
                    .flex_none()
                    .mx(px(8.))
                    .mb(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
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
                                Some(chord) => {
                                    Tooltip::with_keystroke("Settings", chord, window, cx)
                                }
                                None => Tooltip::text("Settings", window, cx),
                            })
                            .child(
                                icons::icon(icons::account::Settings)
                                    .size(px(13.))
                                    .text_color(theme.text_faint),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_settings(Section::General, cx)
                            })),
                    )
                    .child(
                        div().flex().flex_row().items_center().gap(px(2.)).child(
                            theme
                                .ghost("open-project")
                                .px(px(8.))
                                .py(px(6.))
                                .tooltip(move |window, cx| match open_chord.clone() {
                                    Some(chord) => {
                                        Tooltip::with_keystroke("Open project", chord, window, cx)
                                    }
                                    None => Tooltip::text("Open project", window, cx),
                                })
                                .child(
                                    icons::icon(icons::files::FolderPlus)
                                        .size(px(13.))
                                        .text_color(theme.text_faint),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_project_action(&OpenProject, window, cx);
                                })),
                        ),
                    ),
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.rows(cx);
        let head = |row: &Row| matches!(row, Row::Project(_));
        let Some(at) = rows
            .get(..=first)
            .and_then(|above| above.iter().rposition(head))
        else {
            return Empty.into_any_element();
        };
        let Row::Project(ix) = rows[at] else {
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
                    .child(self.project_head(ix, true, cx)),
            )
            .into_any_element()
    }

    /// One project's heading: it folds, and its `+` opens what can be made in
    /// the project.
    ///
    /// `pinned` is the copy [`Cydonia::pinned_head`] holds at the top of the
    /// list. It gives up the pill for the column's full width, and takes the
    /// glass the floating cluster below it is cut from — a heading with rows
    /// running under it has to be read against whatever is passing.
    fn project_head(&self, ix: usize, pinned: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (name, expanded) = match self.workspace.read(cx).projects.get(ix) {
            Some(project) => (project.name(), project.expanded),
            None => return Empty.into_any_element(),
        };
        let carried = SharedString::from(name.clone());
        // Both buttons stay on show while either one's menu is open. They are
        // revealed by the row's hover, and the pointer leaves the row the
        // moment it reaches the card — which took the `+` away from under a
        // menu standing open beside it.
        let held = matches!(self.menu, Some(Menu::Add(at) | Menu::Project(at)) if at == ix);
        let reveal = (!held).then_some("project-head");
        let head = div()
            .id(("project", ix))
            .group("project-head")
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.sidebar_hover(Menu::Add(ix), *hovered, cx);
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
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            // On the head, not the label: a name's colour is fixed when
            // its text is laid out, and only this div is stateful enough
            // to carry the hover that far.
            .text_color(theme.text_faint)
            .hover(|el| el.text_color(theme.text))
            .child(
                theme
                    .ghost(("project-fold", ix))
                    .flex_none()
                    .p(px(2.))
                    .child(
                        icons::icon(match expanded {
                            true => icons::files::FolderOpen,
                            false => icons::files::Folder,
                        })
                        .size(px(14.))
                        .text_color(theme.text_faint)
                        .group_hover("project-head", |el| el.text_color(theme.text)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_project(ix, cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Callout)
                    .font_weight(FontWeight::MEDIUM)
                    .child(name),
            )
            // Ahead of the `+`, which is the one that gets pressed: sorting
            // and the rest are settled once and left alone.
            .child(
                self.menu_button(
                    SharedString::from(format!("project-menu-{ix}")),
                    reveal,
                    icons::layout::Ellipsis,
                    Menu::Project(ix),
                    cx,
                )
                // On the trigger, not the row: the card pins to the bottom
                // left of whatever it is mounted on, and from the row it hangs
                // off the far side of the sidebar rather than under the `···`.
                .children(self.project_menu(ix, cx)),
            )
            .child(
                self.menu_button(
                    SharedString::from(format!("project-add-{ix}")),
                    reveal,
                    icons::math::Plus,
                    Menu::Add(ix),
                    cx,
                )
                .children(self.add_menu(ix, cx)),
            )
            // A press on the copy is a press on where it came from: the list
            // goes back to the heading it is standing in for, rather than
            // folding away the project you are reading. Its own folder mark
            // still folds — that press stops before it reaches here.
            .on_click(cx.listener(move |this, _, _, cx| match pinned {
                true => this.scroll_to_project(ix, cx),
                false => this.toggle_project(ix, cx),
            }))
            // Carried by its heading, and dropped on the heading it is to sit
            // in front of. What the entries under it are ordered by is when
            // they were last written, so they are not reordered by dragging —
            // an entry is dragged onto a pane, not up the column.
            .on_drag(ProjectDrag(ix), move |_, _, _, cx| {
                let carried = carried.clone();
                cx.new(|_| Carried(carried))
            })
            .drag_over::<ProjectDrag>(move |style, _, _, cx| style.bg(Theme::of(cx).element_active))
            .on_drop(cx.listener(move |this, drag: &ProjectDrag, _, cx| {
                this.move_project(drag.0, ix, cx);
            }));
        // Its own menu opens on the press rather than the click, so the note
        // has to be here too — read stale, a right press would swallow.
        let head = self.menu_press(head, Menu::Project(ix), cx);
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
    /// you are not. Only the addresses are ordered — each kind's own list keeps
    /// the indices these carry.
    pub(crate) fn entries(&self, project: usize, cx: &App) -> Vec<Row> {
        let features = &self.workspace.read(cx).settings.features;
        let mut entries = self.ranked(project, cx);
        // A project is read off disk whole whatever is switched on, so what a
        // switch hides it hides here — the entries stay in the project and in
        // memory, and turning it back on lists them again with nothing to
        // rescan.
        entries.retain(|entry| shown(entry.row, features));
        let split = entries.iter().position(|entry| entry.archived);
        let mut rows: Vec<Row> = entries
            .iter()
            .take(split.unwrap_or(entries.len()))
            .map(|entry| entry.row)
            .collect();
        if let Some(split) = split {
            rows.push(Row::Archive(project));
            if self
                .workspace
                .read(cx)
                .projects
                .get(project)
                .is_some_and(|open| open.archive_open)
            {
                rows.extend(entries[split..].iter().map(|entry| entry.row));
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
        let sessions = open.sessions.iter().map(|chat| Ranked {
            archived: chat.closed,
            touched: chat.touched(),
            name: folded(&chat.label()),
            row: Row::Session {
                project,
                id: chat.id,
            },
        });
        let boards = open.boards.iter().enumerate().map(|(ix, board)| Ranked {
            archived: board.archived,
            touched: board.touched,
            name: folded(board.label()),
            row: Row::Board { project, ix },
        });
        let articles = open
            .articles
            .iter()
            .enumerate()
            .map(|(ix, article)| Ranked {
                archived: article.archived,
                touched: article.touched,
                name: folded(article.label()),
                row: Row::Article { project, ix },
            });
        let tables = open.tables.iter().enumerate().map(|(ix, table)| Ranked {
            archived: table.archived,
            // The store keeps seconds; every other stamp here is milliseconds.
            touched: table.updated_at.unwrap_or(table.created_at).max(0) as u128 * 1000,
            name: folded(&table.name),
            row: Row::Table { project, ix },
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
            let pin =
                showing_of(entry.row).and_then(|showing| workspace.pin_rank(project, showing));
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
                        let at = showing_of(entry.row)
                            .and_then(|showing| workspace.rank_of(project, showing));
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
                self.member_of_row(*row, cx)
                    .and_then(|member| workspace.space_holding(&member))
                    .is_none()
            })
            .collect()
    }

    /// Every space, and under each the rows it holds.
    ///
    /// At the end of the list rather than inside a project: a space can hold
    /// panes from several, so it belongs to none of them — see
    /// [`crate::model::spaces`].
    fn space_rows(&self, cx: &App) -> Vec<Row> {
        let workspace = self.workspace.read(cx);
        let mut rows = Vec::new();
        if !workspace.spaces.is_empty() {
            rows.push(Row::Spaces);
        }
        for (ix, space) in workspace.spaces.iter().enumerate() {
            rows.push(Row::Space(ix));
            if self.collapsed_spaces.contains(&space.id) {
                continue;
            }
            // In the order the arrangement lays the panes out, so the list
            // reads across the window.
            //
            // Only what this space is the holder of. One entry is in one
            // space at a time — see [`Workspace::arrange`] — and the same
            // answer decides both lists, so a file written before that held
            // lists its entry once here rather than twice, and never beside the
            // copy [`Self::ungrouped`] took out of the project.
            rows.extend(
                space
                    .entries()
                    .iter()
                    .filter(|member| workspace.space_holding(member) == Some(ix))
                    .filter_map(|member| self.row_of_member(member, cx)),
            );
        }
        rows
    }

    /// Fold a space's members away, or bring them back.
    pub(crate) fn fold_space(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.collapsed_spaces.remove(id) {
            self.collapsed_spaces.insert(id.to_owned());
        }
        cx.notify();
    }

    /// How far in a row is drawn: one step for a project's entries when that
    /// is switched on, and one more for a row an open space holds.
    ///
    /// The indent is the whole of what says a row belongs to the space above
    /// it. Nothing else is drawn on it — a rule beside it or a wash behind it
    /// is a second way of saying what the offset already says.
    pub(crate) fn indent_of(&self, row: Row, cx: &App) -> u8 {
        let workspace = self.workspace.read(cx);
        let base = u8::from(workspace.indent_project_rows);
        match row {
            // A space is not inside a project — it can hold panes from
            // several — so its row starts at the column's edge, where the
            // project headings are.
            Row::Space(_) | Row::Spaces => return 0,
            Row::Project(_) | Row::Archive(_) => return base,
            _ => {}
        }
        let held = || -> Option<()> {
            let member = self.member_of_row(row, cx)?;
            let at = workspace.space_holding(&member)?;
            let space = workspace.spaces.get(at)?;
            (!self.collapsed_spaces.contains(&space.id)).then_some(())
        };
        // One step under the space holding it, and one only: a row listed
        // there is not also under its project's heading, so the project's own
        // indent is not another step to add to this one.
        match held().is_some() {
            true => 1,
            false => base,
        }
    }

    /// The row for a member a space holds — the way back from what it names
    /// to the line that stands for it. Nothing where its project is not open.
    fn row_of_member(&self, member: &Member, cx: &App) -> Option<Row> {
        let (project, showing) = self.workspace.read(cx).showing_of(member)?;
        Some(match showing {
            Showing::Session(id) => Row::Session { project, id },
            Showing::Board(ix) => Row::Board { project, ix },
            Showing::Article(ix) => Row::Article { project, ix },
            Showing::Table(ix) => Row::Table { project, ix },
        })
    }

    /// Every line the sidebar shows, in order. Addresses only: a project with a
    /// thousand articles costs a thousand `Row`s here and reads a title for
    /// none of them.
    pub(crate) fn rows(&self, cx: &Context<Self>) -> Vec<Row> {
        let mut rows = Vec::new();
        for p in 0..self.workspace.read(cx).projects.len() {
            rows.push(Row::Project(p));
            if self.workspace.read(cx).projects[p].expanded {
                rows.extend(self.entries(p, cx));
            }
        }
        // After the projects, because a space is not inside one.
        rows.extend(self.space_rows(cx));
        rows
    }

    /// Open what a row points at — what a keyboard step does with its landing.
    /// The pointer never comes through here: each row carries its own
    /// `on_click`, which needs no [`Row`] to know what it is.
    pub(crate) fn open_row(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        match row {
            Row::Project(ix) => self.select_project(ix, cx),
            Row::Archive(ix) => self.toggle_archive(ix, cx),
            Row::Session { id, .. } => self.select_session(id, window, cx),
            Row::Board { project, ix } => self.open_board(project, ix, window, cx),
            Row::Article { project, ix } => self.open_article(project, ix, window, cx),
            Row::Table { project, ix } => self.open_table(project, ix, window, cx),
            Row::Space(ix) => self.open_space(ix, window, cx),
            // A heading over the spaces, and nothing to open.
            Row::Spaces => {}
        }
    }

    /// Take the list back to where a project starts, heading and all.
    fn scroll_to_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(at) = self
            .rows(cx)
            .iter()
            .position(|row| *row == Row::Project(ix))
        else {
            return;
        };
        self.rail.scroll_to_item(at, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scroll the rail to a row, if it is not already on screen. `Nearest`
    /// rather than `Top`: a step to the neighbour below should move the list by
    /// a row, not throw the one you came from off the top of it.
    pub(crate) fn reveal(&mut self, row: Row, cx: &Context<Self>) {
        if let Some(ix) = self.rows(cx).iter().position(|at| *at == row) {
            self.rail.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    /// One line, built when the list scrolls it into view. The box around it is
    /// what holds the pitch: the row inside paints the wash, and the pixel
    /// either side of it is the gap between two.
    fn sidebar_row(&self, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let workspace = self.workspace.read(cx);
        let inner = match row {
            Row::Project(ix) => self.project_head(ix, false, cx),
            Row::Archive(ix) => self.archive_divider(ix, cx),
            Row::Session { project, id } => match self.session_of(project, id, cx) {
                Some(session) => self.session_row(session, cx),
                None => Empty.into_any_element(),
            },
            Row::Board { project, ix } => {
                match workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.boards.get(ix).map(|board| board.label().to_owned()))
                {
                    Some(name) => self.board_row(project, ix, name, cx),
                    None => Empty.into_any_element(),
                }
            }
            Row::Article { project, ix } => {
                match workspace.projects.get(project).and_then(|open| {
                    open.articles
                        .get(ix)
                        .map(|article| article.label().to_owned())
                }) {
                    Some(title) => self.article_row(project, ix, title, cx).into_any_element(),
                    None => Empty.into_any_element(),
                }
            }
            Row::Table { project, ix } => {
                match workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.tables.get(ix).map(|table| table.name.clone()))
                {
                    Some(name) => self.table_row(project, ix, name, cx).into_any_element(),
                    None => Empty.into_any_element(),
                }
            }
            Row::Space(ix) => self.space_row(ix, cx),
            Row::Spaces => self.spaces_label(cx),
        };
        let carried = self.drag_of_row(row, cx);
        let label = SharedString::from(self.label_of_row(row, cx));
        let entry = !matches!(row, Row::Project(_) | Row::Archive(_) | Row::Spaces);
        let archived = self.archived_of(row, cx);
        div()
            .id(SharedString::from(format!("sidebar-hover-{}", key_of(row))))
            .when(entry, |el| {
                el.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.sidebar_hover(Menu::Entry(row), *hovered, cx);
                }))
                // Every kind of row from one place, and the menu drawn here
                // rather than under whatever the row ends in: a right press
                // lands wherever the pointer is, and the trigger it opens from
                // is the row.
                .relative()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _, _, cx| {
                        this.toggle_menu(Menu::Entry(row), cx);
                    }),
                )
                .children(
                    (!self.pinned(row, cx))
                        .then(|| self.entry_menu(Menu::Entry(row), row, archived, cx))
                        .flatten(),
                )
            })
            // Carried onto a pane's edge to put it beside what is there — see
            // [`crate::view::arrangement`].
            .when_some(carried, |el, carried| {
                el.on_drag(carried, move |_, _, _, cx| {
                    let label = label.clone();
                    cx.new(|_| Carried(label))
                })
            })
            // And dropped on the row it is to take the place of, the way a
            // project heading is.
            .when(showing_of(row).is_some(), |el| {
                el.drag_over::<EntryDrag>(move |style, _, _, cx| {
                    style.bg(Theme::of(cx).element_active)
                })
                .on_drop(cx.listener(move |this, drag: &EntryDrag, _, cx| {
                    if let Some(carried) = this.dropped(drag, cx) {
                        this.reorder_entry(&carried, row, cx);
                    }
                }))
            })
            .h(px(ROW_HEIGHT))
            .py(px(1.))
            .child(inner)
            .into_any_element()
    }

    /// What the row is called, for the ghost that follows the pointer.
    fn label_of_row(&self, row: Row, cx: &Context<Self>) -> String {
        let workspace = self.workspace.read(cx);
        let named = || -> Option<String> {
            Some(match row {
                Row::Project(_) | Row::Archive(_) | Row::Spaces => return None,
                Row::Space(ix) => workspace.spaces.get(ix)?.label().to_owned(),
                Row::Session { project, id } => {
                    workspace.projects.get(project)?.session(id)?.label()
                }
                Row::Board { project, ix } => workspace
                    .projects
                    .get(project)?
                    .boards
                    .get(ix)?
                    .label()
                    .to_owned(),
                Row::Article { project, ix } => workspace
                    .projects
                    .get(project)?
                    .articles
                    .get(ix)?
                    .label()
                    .to_owned(),
                Row::Table { project, ix } => workspace
                    .projects
                    .get(project)?
                    .tables
                    .get(ix)?
                    .name
                    .clone(),
            })
        };
        named().unwrap_or_default()
    }

    /// Whether a space is what the window is showing.
    ///
    /// While one is, the sidebar lights its row and no other: the entries it
    /// arranges are listed as themselves, and lighting them too would leave
    /// the column with no one row that says what is open.
    pub(crate) fn arranged(&self, cx: &App) -> bool {
        self.workspace.read(cx).active_space().is_some()
    }

    /// Put the carried entry where `onto` is, and write the project's order
    /// down.
    ///
    /// The whole list is rewritten rather than the one row that moved: an
    /// order held as gaps between the rows that did move is one every later
    /// read has to reconstruct, and the list on screen is already the answer.
    ///
    /// A drag never pins or unpins. The pins are a region of the list with
    /// their own order, and a row dragged between the two regions would be
    /// changing what it *is* rather than where it sits — that is what the
    /// button at the end of the row and the band's `···` are for.
    fn reorder_entry(&mut self, carried: &Member, onto: Row, cx: &mut Context<Self>) {
        let Some(project) = project_of(onto) else {
            return;
        };
        let rows: Vec<Row> = self
            .ranked(project, cx)
            .into_iter()
            .map(|entry| entry.row)
            .collect();
        let from = rows
            .iter()
            .position(|row| self.member_of_row(*row, cx).as_ref() == Some(carried));
        let to = rows.iter().position(|row| *row == onto);
        let (Some(from), Some(to)) = (from, to) else {
            return;
        };
        if from == to || self.pinned(rows[from], cx) != self.pinned(onto, cx) {
            return;
        }
        let among_pins = self.pinned(onto, cx);
        let mut moved = rows;
        let row = moved.remove(from);
        moved.insert(to, row);
        let workspace = self.workspace.read(cx);
        let entry_of = |row: &Row| workspace.entry_of(project, showing_of(*row)?);
        // Every entry, so that unpinning one later puts it back where it sat
        // rather than at the top.
        let order: Vec<state::Entry> = moved.iter().filter_map(entry_of).collect();
        // And the pins again, when it was one of them that moved: their own
        // order is what the region above is listed by.
        let pins: Option<Vec<state::Entry>> = among_pins.then(|| {
            moved
                .iter()
                .filter(|row| self.pinned(**row, cx))
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
    fn member_of_row(&self, row: Row, cx: &App) -> Option<Member> {
        let showing = showing_of(row)?;
        self.workspace.read(cx).member_of(project_of(row)?, showing)
    }

    /// What a drag off this row carries. A session with no file yet has no
    /// name a space can hold, and is carried as itself — see [`EntryDrag`].
    fn drag_of_row(&self, row: Row, cx: &App) -> Option<EntryDrag> {
        match self.member_of_row(row, cx) {
            Some(member) => Some(EntryDrag::Member(member)),
            None => match row {
                Row::Session { project, id } => Some(EntryDrag::Session { project, id }),
                _ => None,
            },
        }
    }

    /// What the sidebar needs of a session, read when its row comes on screen.
    fn session_of(&self, project: usize, id: u64, cx: &Context<Self>) -> Option<SessionRow> {
        let workspace = self.workspace.read(cx);
        let chat = workspace.projects.get(project)?.session(id)?;
        Some(SessionRow {
            project,
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

    /// The line the spaces are listed under.
    ///
    /// It says what the rows below it are, which the rows cannot: a space is
    /// not inside a project, so what follows the last project would otherwise
    /// read as more of it. Nothing folds here — each space carries its own
    /// fold, and a second one over the group would be two ways to hide a row.
    fn spaces_label(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // Not [`row`]: that carries the pointer and the hover wash, which say
        // a press does something. Nothing answers one here.
        div()
            .h(px(ROW_PILL))
            .mx(px(root::SIDEBAR_GUTTER))
            .px(px(root::SIDEBAR_GUTTER))
            .flex()
            .flex_row()
            .items_center()
            .text_style(TextStyle::Callout)
            .text_color(theme.text_faint)
            .child("Spaces")
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

    /// Menus address a project by its place in the list, so the one open when
    /// it moves would be pointing at whichever project slid underneath.
    fn move_project(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.menu = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.move_project(from, to, cx));
    }

    /// Menus address a space by its place in the list, the way they do a
    /// project — see [`Self::move_project`].
    fn move_space(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.menu = None;
        self.workspace
            .update(cx, |workspace, cx| workspace.move_space(from, to, cx));
    }

    fn toggle_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.toggle_project(ix, cx));
    }

    /// What the `+` starts here. Session first: it is what the sidebar is for.
    ///
    /// With more than one agent installed the session row asks which, since
    /// the one `⌘N` would pick is whoever the project last talked to — which
    /// leaves every other agent with no way in.
    fn add_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Add(ix)) {
            return None;
        }
        let workspace = self.workspace.read(cx);
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
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// What a press on the heading opens. Removing closes the tab — the
    /// directory and everything in it stays where it is.
    fn project_menu(&self, ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.menu != Some(Menu::Project(ix)) {
            return None;
        }
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
        // Finder is the one file manager this knows how to ask.
        #[cfg(target_os = "macos")]
        rows.push(menu::row(
            Item::action("Reveal in Finder").with_icon(icons::files::FolderOpen),
            move |this, _, cx| this.reveal_project(ix, cx),
        ));
        rows.push(menu::row(
            Item::action("Remove project").with_icon(icons::files::FolderMinus),
            move |this, _, cx| this.close_project(ix, cx),
        ));
        let id = SharedString::from(format!("project-menu-{ix}"));
        Some(popover::anchored_menu_below(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// Show the project's directory in Finder. Best effort and off the main
    /// thread: `open` is a process, and a Finder that will not come to the
    /// front is not worth blocking a frame over.
    #[cfg(target_os = "macos")]
    fn reveal_project(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .workspace
            .read(cx)
            .projects
            .get(ix)
            .map(|open| open.path.clone())
        else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = crate::view::component::file::external::show(&path);
            })
            .detach();
    }

    /// One session: its mark and its name.
    fn session_row(&self, session: SessionRow, cx: &mut Context<Self>) -> AnyElement {
        let entry = Row::Session {
            project: session.project,
            id: session.id,
        };
        let theme = Theme::of(cx).clone();
        let id = session.id;
        let selected = !self.arranged(cx)
            && self.showing(cx) == Some(Pane::Chat)
            && self.workspace.read(cx).active_id() == Some(id);
        let tint = tint(selected, session.archived, &theme);
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
            format!("session-archive-{id}"),
            "session-row",
            entry,
            session.archived,
            cx,
        ))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.select_session(id, window, cx);
        }))
        .into_any_element()
    }

    /// One space: the arrangement, and how many panes it holds.
    ///
    /// While it is open its members are listed under it rather than under their
    /// projects — see [`Self::ungrouped`].
    fn space_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = workspace.space == Some(ix);
        let entry = Row::Space(ix);
        let Some((name, id)) = workspace
            .spaces
            .get(ix)
            .map(|space| (space.label().to_owned(), space.id.clone()))
        else {
            return Empty.into_any_element();
        };
        let indent = self.indent_of(entry, cx);
        // A heading rather than another entry row. Nothing is drawn on the
        // rows it holds — a rule has nothing to mark without an indent to run
        // down, and a wash behind the group reads as a second row state. What
        // says they belong to it is that it is written as a heading and they
        // are directly under it, which is what `Archived` above them does and
        // what the sidebar already reads as.
        let tint = match selected {
            true => tint(selected, false, &theme),
            false => theme.text_faint,
        };
        let name_of_row = name.clone();
        let label = match matches!(&self.renaming, Some(Renaming::Space(at)) if *at == id) {
            true => self.name_field(cx),
            false => row_heading(name, tint),
        };
        let folded = self.collapsed_spaces.contains(&id);
        let held = id.clone();
        let carried = SharedString::from(name_of_row.clone());
        row(("space", ix), "space-row", selected, indent, &theme)
            // The mark carries the fold, the way a project's folder does
            // rather than standing a chevron beside it: open, the panes it
            // holds are listed under it; closed, it is the arrangement alone.
            .child(
                div()
                    .id(("space-fold", ix))
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(
                        icons::icon(match folded {
                            true => icons::layout::LayoutDashboard,
                            false => icons::layout::LayoutFreeform,
                        })
                        .size(px(14.))
                        .text_color(tint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Only the mark: a press on the row itself opens the
                        // space, which is the other thing the row is for.
                        cx.stop_propagation();
                        this.fold_space(&held, cx);
                    })),
            )
            .child(label)
            // A space has no archive button of its own: putting one away
            // takes its members with it, which is a `···` decision rather than
            // a press in passing. Rename lives here too — a space has no band
            // to be renamed in, since the window draws none while one is open.
            .child(
                self.menu_button(
                    SharedString::from(format!("space-menu-{ix}")),
                    None,
                    icons::layout::Ellipsis,
                    Menu::Entry(entry),
                    cx,
                )
                .flex_none()
                .when(
                    self.sidebar_hovered.as_ref() != Some(&Menu::Entry(entry))
                        && self.menu.as_ref() != Some(&Menu::Entry(entry)),
                    |el| el.hidden(),
                ),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.open_space(ix, window, cx)))
            // Carried by its row and dropped on the row it is to sit in front
            // of, the way a project heading is — see [`Self::project_head`].
            .on_drag(SpaceDrag(ix), move |_, _, _, cx| {
                let carried = carried.clone();
                cx.new(|_| Carried(carried))
            })
            .drag_over::<SpaceDrag>(move |style, _, _, cx| style.bg(Theme::of(cx).element_active))
            .on_drop(cx.listener(move |this, drag: &SpaceDrag, _, cx| {
                this.move_space(drag.0, ix, cx);
            }))
            .into_any_element()
    }

    /// One board: its mark and its name.
    fn board_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = !self.arranged(cx)
            && self.showing(cx) == Some(Pane::Board)
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.board == Some(ix));
        let entry = Row::Board { project, ix };
        let board = workspace
            .projects
            .get(project)
            .and_then(|open| open.boards.get(ix));
        let archived = board.is_some_and(|board| board.archived);
        let tint = tint(selected, archived, &theme);
        // No inline field on a board's row, ever: a board is named by its panel
        // — see [`Self::rename_entry`].
        let label = row_label(name, tint);

        row(
            SharedString::from(format!("board-{project}-{ix}")),
            "board-row",
            selected,
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
            SharedString::from(format!("board-archive-{project}-{ix}")),
            "board-row",
            entry,
            archived,
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
        entry: Row,
        archived: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let id = id.into();
        let pinned = self.pinned(entry, cx);
        if pinned {
            let at = Menu::Entry(entry);
            // The glyph alone turns over; the button is the same button either
            // way, so a press during the frame the hover is still travelling
            // opens the menu rather than falling through to the row.
            let open = self.menu.as_ref() == Some(&at);
            let mark = match open || self.sidebar_hovered.as_ref() == Some(&at) {
                true => icons::layout::Ellipsis,
                false => icons::navigation::Pin,
            };
            return self
                .menu_button(
                    id,
                    None,
                    mark,
                    at.clone(),
                    cx,
                )
                // On the trigger, so the card hangs under the `···` rather
                // than off the left edge of the row it is mounted on. A row
                // with no button of its own draws it from the wrapper, where
                // a right press is all there is to anchor to.
                .children(self.entry_menu(at, entry, archived, cx));
        }
        let held = self.menu.as_ref() == Some(&Menu::Entry(entry));
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
                this.archive_entry(entry, !archived, window, cx);
            }))
    }

    /// Whether a row's entry is put away. `false` for the rows that are not
    /// entries, and for a space — archiving one drops the arrangement rather
    /// than filing it.
    fn archived_of(&self, row: Row, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        match row {
            Row::Session { project, id } => workspace
                .projects
                .get(project)
                .and_then(|open| open.session(id))
                .is_some_and(|chat| chat.closed),
            Row::Board { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.boards.get(ix))
                .is_some_and(|board| board.archived),
            Row::Article { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.articles.get(ix))
                .is_some_and(|article| article.archived),
            Row::Table { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.tables.get(ix))
                .is_some_and(|table| table.archived),
            Row::Space(_) | Row::Project(_) | Row::Archive(_) | Row::Spaces => false,
        }
    }

    /// Whether an entry is held at the top of its project's list.
    pub(crate) fn pinned(&self, entry: Row, cx: &App) -> bool {
        let (Some(project), Some(showing)) = (project_of(entry), showing_of(entry)) else {
            return false;
        };
        self.workspace.read(cx).is_pinned(project, showing)
    }

    /// Pin an entry to the top of its project's list, or let it back down.
    pub(crate) fn pin_entry(&mut self, entry: Row, on: bool, cx: &mut Context<Self>) {
        let (Some(project), Some(showing)) = (project_of(entry), showing_of(entry)) else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.pin(project, showing, on, cx));
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
        entry: Row,
        archived: bool,
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
        let named = !matches!(entry, Row::Article { .. } | Row::Board { .. });
        let mut rows = vec![menu::row(put, move |this, window, cx| {
            this.archive_entry(entry, !archived, window, cx)
        })];
        // Above archive, and only for an entry still in hand: what is put away
        // is not held at the top of anything.
        if !archived && showing_of(entry).is_some() {
            let pinned = self.pinned(entry, cx);
            let pin = match pinned {
                true => Item::action("Unpin").with_icon(icons::navigation::PinOff),
                false => Item::action("Pin to top").with_icon(icons::navigation::Pin),
            };
            rows.insert(
                0,
                menu::row(pin, move |this, _, cx| this.pin_entry(entry, !pinned, cx)),
            );
        }
        if named {
            rows.insert(
                0,
                menu::row(
                    Item::action("Rename").with_icon(icons::text::SquarePen),
                    move |this, window, cx| this.rename_entry(entry, window, cx),
                ),
            );
        }
        // A page's measure and how it is being read: the open page's, since
        // [`Self::set_full_width`] and [`Self::plain_text`] are about the one
        // the window is showing. A row's menu names an entry that may not be
        // it, so these are the band's alone — on the wrong row they would act
        // on whatever else was open.
        if matches!(entry, Row::Article { .. }) && !matches!(at, Menu::Entry(_)) {
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
        rows.push(menu::row(
            Item::action("Delete").with_icon(icons::files::Trash),
            move |this, _, cx| this.ask_delete(entry, cx),
        ));
        let id = SharedString::from("header-menu-card");
        // Right-aligned: every route into this menu — the dots button, the
        // pin, a right press — has its affordance at the row's end, and the
        // card drops from there.
        Some(popover::anchored_menu_below_end(
            id.clone(),
            self.menu_card(id, rows, cx),
            None,
        ))
    }

    /// Drop an entry, file and all. Deleting the session on screen lands on
    /// the first remaining entry in the sidebar's displayed order.
    pub(crate) fn delete_entry(&mut self, entry: Row, window: &mut Window, cx: &mut Context<Self>) {
        let landing_project = match entry {
            Row::Session { project, id }
                if self.showing(cx) == Some(Pane::Chat)
                    && self.workspace.read(cx).active == Some(project)
                    && self.workspace.read(cx).active_id() == Some(id) =>
            {
                Some(project)
            }
            _ => None,
        };
        self.commit(cx);
        self.workspace.update(cx, |workspace, cx| match entry {
            Row::Session { id, .. } => workspace.close_session(id, cx),
            Row::Board { project, ix } => workspace.delete_board(project, ix, cx),
            Row::Article { project, ix } => workspace.delete_article(project, ix, cx),
            Row::Table { project, ix } => workspace.delete_table(project, ix, cx),
            Row::Space(ix) => workspace.delete_space(ix, cx),
            Row::Project(_) | Row::Archive(_) | Row::Spaces => {}
        });
        if let Some(project) = landing_project
            && let Some(landing) = self
                .entries(project, cx)
                .into_iter()
                .find(|row| !matches!(row, Row::Archive(_)))
        {
            self.open_row(landing, window, cx);
            self.reveal(landing, cx);
        }
        cx.notify();
    }

    /// Put the name field on an entry's row, for the kinds named that way.
    /// Each is addressed by what identifies it, so the field cannot slide onto
    /// its neighbour if the list reorders under it.
    ///
    /// A board is named by two things at once, so it opens its identity panel
    /// instead — which lives under the band, so the board is brought to the
    /// front first. One way to name a board, wherever you asked from.
    fn rename_entry(&mut self, entry: Row, window: &mut Window, cx: &mut Context<Self>) {
        if let Row::Board { project, ix } = entry {
            let id = self
                .workspace
                .read(cx)
                .projects
                .get(project)
                .and_then(|open| open.boards.get(ix))
                .map(|board| board.id.clone());
            if let Some(id) = id {
                self.open_board(project, ix, window, cx);
                self.open_info(&id, window, cx);
            }
            return;
        }
        let workspace = self.workspace.read(cx);
        let what = match entry {
            Row::Session { id, .. } => Some(Renaming::Session(id)),
            Row::Table { project, ix } => workspace
                .projects
                .get(project)
                .and_then(|open| open.tables.get(ix))
                .map(|table| Renaming::Table(table.key.clone())),
            // An article is named in its own page, and the two that are not
            // entries have no name to take.
            Row::Space(ix) => workspace
                .spaces
                .get(ix)
                .map(|space| Renaming::Space(space.id.clone())),
            Row::Article { .. }
            | Row::Board { .. }
            | Row::Project(_)
            | Row::Archive(_)
            | Row::Spaces => None,
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
        entry: Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read before the flag moves: once the entry is away it is no longer
        // what any pane is on.
        let landing = (archived && self.in_front(entry, cx))
            .then(|| project_of(entry))
            .flatten();
        // A space takes its members with it. Membership is exclusive — an
        // entry is in one space at a time — so the arrangement owns what it
        // holds, and putting it away that holds nothing would be putting away
        // an empty row.
        //
        // The arrangement itself is dropped rather than put away: a space
        // names entries and holds none, so there is nothing in one to come
        // back to, and it is remade by dragging one entry onto another. By id,
        // because archiving a member takes it out of the space as it goes —
        // see [`Workspace::drop_from_spaces`] — and that shifts every index
        // past it.
        if let Row::Space(ix) = entry {
            let workspace = self.workspace.read(cx);
            let Some(space) = workspace.spaces.get(ix) else {
                return;
            };
            let id = space.id.clone();
            let members: Vec<Row> = space
                .entries()
                .iter()
                .filter_map(|member| self.row_of_member(member, cx))
                .collect();
            for member in members {
                self.archive_entry(member, archived, window, cx);
            }
            self.workspace
                .update(cx, |workspace, cx| workspace.delete_space_id(&id, cx));
            return;
        }
        // Putting an entry away takes it out of the two places that hold it
        // up: the pins at the top of the list, and whatever space arranges
        // it. Both are about an entry in hand, and this one no longer is.
        if archived && let Some(showing) = showing_of(entry) {
            let member = self.member_of_row(entry, cx);
            self.workspace.update(cx, |workspace, cx| {
                if let Some(project) = project_of(entry) {
                    workspace.unpin_entry(project, showing);
                }
                if let Some(member) = member {
                    workspace.drop_from_spaces(&member, cx);
                }
            });
        }
        self.workspace.update(cx, |workspace, cx| match entry {
            Row::Space(_) => {}
            Row::Session { id, .. } => workspace.archive_session(id, archived, cx),
            Row::Board { project, ix } => {
                if let Some(id) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.boards.get(ix))
                    .map(|board| board.id.clone())
                {
                    workspace.archive_board(&id, archived, cx);
                }
            }
            Row::Article { project, ix } => {
                if let Some(path) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.articles.get(ix))
                    .map(|article| article.path.clone())
                {
                    workspace.archive_article(&path, archived, cx);
                }
            }
            Row::Table { project, ix } => {
                if let Some(key) = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.tables.get(ix))
                    .map(|table| table.key.clone())
                {
                    workspace.archive_table(&key, archived, cx);
                }
            }
            Row::Project(_) | Row::Archive(_) | Row::Spaces => {}
        });
        if let Some(project) = landing {
            self.open_top_entry(project, window, cx);
        }
    }

    /// Whether the pane in front is on this entry.
    fn in_front(&self, entry: Row, cx: &App) -> bool {
        let (Some(project), Some(showing)) = (project_of(entry), showing_of(entry)) else {
            return false;
        };
        let workspace = self.workspace.read(cx);
        let Some(open) = workspace.projects.get(project) else {
            return false;
        };
        if workspace.active != Some(project) || self.arranged(cx) {
            return false;
        }
        let pane = self.showing(cx);
        match showing {
            Showing::Session(id) => pane == Some(Pane::Chat) && open.active == Some(id),
            Showing::Board(ix) => pane == Some(Pane::Board) && open.board == Some(ix),
            Showing::Article(ix) => pane == Some(Pane::Article) && open.article == Some(ix),
            Showing::Table(ix) => pane == Some(Pane::Table) && open.table == Some(ix),
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
            .find(|row| showing_of(*row).is_some())
        else {
            return;
        };
        self.open_row(top, window, cx);
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
