//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it, drawn by its own [`Sidebar`] entity under the band
//! the window draws at the top of the column.

mod list;
mod rows;

pub(crate) use list::List;

use crate::{
    model::{
        project::ProjectId,
        session::{ChatSession, Status},
        settings::Features,
        workspace::{Member, Showing, Workspace},
    },
    view::{
        kit::{
            menu::{self, Act, MenuHost},
            rename::{Rename, Renaming},
            status,
            watch::{FocusWatch, Watch},
        },
        search::Search,
        window::{
            reach::InWindow,
            root::{self, Cydonia},
        },
    },
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Div, Empty, Entity, FocusHandle, Hsla, ListAlignment,
        ListOffset, ListState, MouseButton, SharedString, Stateful, Subscription, Task, WeakEntity,
        Window, div, prelude::*, px,
    },
    theme::{Selection, TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place as MenuPlace},
        drag::{self, Carried},
        icons::{self, Icon},
        menu::Item,
        scroll as scrollbars,
    },
};
use std::{collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

/// The wash a row paints. With 1px either side it is [`ROW_HEIGHT`], the
/// pitch of headings and rows alike.
const ROW_PILL: f32 = 30.;

/// What a row puts between its mark, its name and the button at the end.
const ROW_GAP: f32 = 8.;

/// The button at the end of a row, at its full size: the height of the row's
/// `···` icon button, which it swaps with. It is laid out at no width until the
/// pointer arrives — see [`Sidebar::archive_button`].
const BUTTON_SIZE: f32 = Theme::BUTTON_HEIGHT;

pub(crate) const ROW_HEIGHT: f32 = ROW_PILL + 2.;

/// An extended entry's name line, and each line of text under it.
const TITLE_LINE: f32 = 18.;
const DETAIL_LINE: f32 = 16.;

/// What an extended entry keeps above its name and below its last line.
const EXTENDED_PAD: f32 = 5.;

/// The wash an extended entry with no labels paints: its name and its
/// preview. One with labels adds a [`DETAIL_LINE`] for them.
const EXTENDED_PILL: f32 = 2. * EXTENDED_PAD + TITLE_LINE + DETAIL_LINE;

/// How far past the visible rows the list lays out.
const OVERDRAW: f32 = 10. * ROW_HEIGHT;

/// How often an extended sidebar redraws, so how long ago each entry was
/// touched stays true.
const TICK: Duration = Duration::from_secs(60);

/// How far the pinned heading's glass runs past the band it is seen in, and is
/// clipped away.
const PINNED_BLEED: f32 = 20.;

/// One step in from the row above, for each level a row is under.
const INDENT_STEP: f32 = 14.;

/// What the sidebar needs of a session to draw its row, read out of the model
/// before the row is built: a turn in flight puts a thinking orb in the mark's
/// place, and the orb leases the frame clock, which wants the app mutably.
struct SessionRow {
    id: u64,
    icon: Option<Icon>,
    /// Its status's mark. `None` while it is idle, which is what puts the
    /// agent's own mark back.
    status: Option<status::Mark>,
}

/// What the sidebar shows of a session — what moving redraws it.
#[derive(Clone, PartialEq)]
struct Listed {
    label: String,
    status: Status,
    closed: bool,
    labels: Vec<String>,
    number: Option<u64>,
    /// Read only while the rows are extended, which is where it is drawn.
    preview: Option<String>,
}

impl Listed {
    fn of(chat: &ChatSession, extended: bool) -> Self {
        Self {
            label: chat.label(),
            status: chat.status(),
            closed: chat.closed,
            labels: chat.labels.clone(),
            number: chat.number,
            preview: extended.then(|| chat.preview()),
        }
    }
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
    /// The line the archived entries of this project are folded under.
    Archive(ProjectId),
    /// An entry of `project`.
    Entry {
        project: ProjectId,
        showing: Showing,
    },
    /// An app, by its id.
    App(String),
}

/// The rows the sidebar lists and what is read off them, index for index.
#[derive(Clone, Default)]
struct Listing {
    rows: Rc<Vec<Row>>,
    shapes: Rc<Vec<Shape>>,
    places: Rc<HashMap<Row, Place>>,
    /// Whether each row is listed by a label filter, and so not dragged.
    narrowed: Rc<Vec<bool>>,
}

/// How a row is drawn, which is what its height follows.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    /// One line: every heading, and every entry while the rows are not
    /// extended.
    Line,
    /// An entry's name over its preview, and its labels under them where it
    /// has any.
    Extended { labelled: bool },
}

impl Shape {
    /// The height the row takes in the list, the gap either side included.
    pub(crate) fn pitch(self) -> f32 {
        self.pill() + 2.
    }

    /// The height of the wash the row paints.
    pub(crate) fn pill(self) -> f32 {
        match self {
            Shape::Line => ROW_PILL,
            Shape::Extended { labelled: false } => EXTENDED_PILL,
            Shape::Extended { labelled: true } => EXTENDED_PILL + DETAIL_LINE,
        }
    }
}

impl Row {
    /// The kind of entry it is. Nothing for a row that is not an entry.
    pub(crate) fn kind(&self) -> Option<Kind> {
        match self {
            Row::Entry { showing, .. } => Some(showing.kind()),
            Row::App(_) => Some(Kind::App),
            _ => None,
        }
    }

    /// What an entry row stands for, and `None` for the rows that are not
    /// entries.
    pub(crate) fn showing(&self) -> Option<&Showing> {
        match self {
            Row::Entry { showing, .. } => Some(showing),
            Row::Group(_) | Row::Archive(_) | Row::Heading(_) | Row::App(_) => None,
        }
    }

    /// Where an entry row's project is listed now, and where its entry is
    /// listed among its kind in that project.
    pub(crate) fn located(&self, workspace: &Workspace) -> Option<(usize, usize)> {
        let Row::Entry { project, showing } = self else {
            return None;
        };
        let at = workspace.project_ix(*project)?;
        Some((at, workspace.projects[at].ix_of(showing)?))
    }

    /// What a space would name this row, so it can be dragged into one.
    pub(crate) fn member(&self, workspace: &Workspace, cx: &App) -> Option<Member> {
        match self {
            Row::Entry { project, showing } => {
                workspace.member_of(workspace.project_ix(*project)?, showing.clone(), cx)
            }
            Row::App(id) => Some(Member::app(id.clone())),
            Row::Group(_) | Row::Archive(_) | Row::Heading(_) => None,
        }
    }

    /// The row for a member a space holds — the way back from what it names
    /// to the line that stands for it. Nothing where its project is not open,
    /// or its app is not listed.
    pub(crate) fn of_member(member: &Member, workspace: &Workspace, cx: &App) -> Option<Self> {
        match member {
            Member::Entry(entry) => {
                let (_, showing) = workspace.showing_of(member, cx)?;
                Some(Row::Entry {
                    project: entry.project,
                    showing,
                })
            }
            Member::App { app } => {
                workspace.program(app, cx)?;
                Some(Row::App(app.clone()))
            }
        }
    }

    /// The member a row names once it lands in a pane. A session carried with
    /// no file yet is given one here: a space names its members by file, so
    /// there is nothing to put in one until this runs. Minting it at the drop
    /// rather than at the drag keeps a gesture that went nowhere from leaving
    /// a session behind on disk.
    pub(crate) fn landed(&self, workspace: &Entity<Workspace>, cx: &mut App) -> Option<Member> {
        if let Some(member) = self.member(workspace.read(cx), cx) {
            return Some(member);
        }
        let Row::Entry {
            project,
            showing: Showing::Session(id),
        } = self
        else {
            return None;
        };
        let id = *id;
        workspace.update(cx, |workspace, cx| {
            workspace.retain_session(id, cx)?;
            workspace.member_of(workspace.project_ix(*project)?, Showing::Session(id), cx)
        })
    }

    /// What the row is called.
    pub(crate) fn label(&self, workspace: &Workspace, cx: &App) -> String {
        let named = || -> Option<String> {
            Some(match self {
                Row::Group(Group::Project(project)) => workspace.project(*project)?.name(),
                Row::Archive(_) | Row::Heading(_) => return None,
                Row::App(id) => workspace.program(id, cx)?.read(cx).label(),
                Row::Group(Group::Space(id)) => workspace.space_by_id(id)?.label().to_owned(),
                Row::Group(Group::Agent(id)) => workspace.identity(id)?.name.clone(),
                Row::Entry { showing, .. } => {
                    let (project, ix) = self.located(workspace)?;
                    workspace.projects[project]
                        .facts_at(showing.kind(), ix, cx)
                        .label
                }
            })
        };
        named().unwrap_or_default()
    }

    /// Whether a row's entry is put away. `false` for the rows that are not
    /// entries, and for a space — archiving one drops the arrangement rather
    /// than filing it.
    pub(crate) fn archived(&self, workspace: &Workspace, cx: &App) -> bool {
        let Row::Entry { showing, .. } = self else {
            return false;
        };
        let Some((project, ix)) = self.located(workspace) else {
            return false;
        };
        workspace.projects[project]
            .facts_at(showing.kind(), ix, cx)
            .archived
    }

    /// Whether an entry is held at the top of its project's list.
    pub(crate) fn pinned(&self, workspace: &Workspace, cx: &App) -> bool {
        let Row::Entry { project, showing } = self else {
            return false;
        };
        workspace
            .project_ix(*project)
            .is_some_and(|project| workspace.is_pinned(project, showing.clone(), cx))
    }

    /// The program an app's row names.
    pub(crate) fn program(
        &self,
        workspace: &Workspace,
        cx: &App,
    ) -> Option<Entity<crate::model::apps::Program>> {
        let Row::App(id) = self else {
            return None;
        };
        workspace.program(id, cx).cloned()
    }

    /// Where an entry row is on disk, for a backend that is the disk.
    pub(crate) fn place(&self, workspace: &Workspace, cx: &App) -> Option<PathBuf> {
        let (project, _) = self.located(workspace)?;
        let open = &workspace.projects[project];
        let (kind, id) = match self.showing()? {
            Showing::Article(id) => (Kind::Article, id.clone()),
            Showing::Board(id) => (Kind::Board, id.clone()),
            Showing::Session(id) => (Kind::Session, open.session(*id, cx)?.filed()?.to_owned()),
            Showing::Table(_) => return None,
        };
        open.store().place(kind, &id)
    }
}

/// The sections of the list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Heading {
    Projects,
    /// The agent identities. Drawn while sessions are switched on.
    Agents,
    /// Drawn only while there is a space.
    Spaces,
    /// The apps listed. Drawn only while there is one.
    Apps,
}

impl Heading {
    /// What the heading is called, on screen and in `state.toml`.
    fn label(self) -> &'static str {
        match self {
            Self::Projects => "Projects",
            Self::Agents => "Agents",
            Self::Spaces => "Spaces",
            Self::Apps => "Apps",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Projects => "projects",
            Self::Agents => "agents",
            Self::Spaces => "spaces",
            Self::Apps => "apps",
        }
    }
}

/// What entries are listed under: a project holds its own, a space the ones it
/// arranges, an agent identity the sessions run as it. One heading and one
/// fold for all three — see [`Sidebar::group_head`].
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Group {
    Project(ProjectId),
    /// A space, by its id.
    Space(String),
    /// An agent identity, by its id.
    Agent(String),
}

/// How a row's entry stands to what the window is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Light {
    /// In the focused pane: the one row that says where you are.
    Focused,
    /// In another tab the window holds.
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

/// What the window is showing, as a row's [`Light`] is read against. The
/// window hands it to the sidebar as it changes.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Lit {
    /// The library stands over every pane.
    pub(crate) library: bool,
    /// The focused tab's entry.
    pub(crate) focused: Option<Member>,
    /// Every leaf the window holds: the arranged space's entries and the
    /// strip's tabs.
    pub(crate) open: Vec<Member>,
}

impl Lit {
    /// Whether the focused tab is on this entry. Never while the library
    /// stands over the panes.
    pub(crate) fn in_front(&self, entry: &Row, workspace: &Workspace, cx: &App) -> bool {
        self.light(entry, workspace, cx) == Light::Focused
    }

    /// How a row's entry stands to what the window is showing — see
    /// [`Light`].
    pub(crate) fn light(&self, row: &Row, workspace: &Workspace, cx: &App) -> Light {
        // The library stands over every pane, so no row is what is on screen.
        if self.library {
            return Light::Off;
        }
        let Some(member) = row.member(workspace, cx) else {
            return Light::Off;
        };
        if self.focused.as_ref() == Some(&member) {
            Light::Focused
        } else if self.open.contains(&member) {
            Light::Shown
        } else {
            Light::Off
        }
    }
}

/// Show `path` in the file manager, and put it on the clipboard.
pub(crate) fn on_disk(path: PathBuf) -> [(Item, Act); 2] {
    let copied = path.to_string_lossy().into_owned();
    [
        menu::row::<Cydonia>(
            Item::action(crate::view::component::file::external::REVEAL)
                .with_icon(icons::files::FolderOpen),
            move |this, window, cx| this.reveal_path(path.clone(), window, cx),
        ),
        menu::row::<Cydonia>(
            Item::action("Copy Path").with_icon(icons::text::Copy),
            move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(copied.clone())),
        ),
    ]
}

/// The project an entry row, a project's heading or its archive line belongs
/// to.
fn project_of(row: &Row) -> Option<ProjectId> {
    match row {
        Row::Group(Group::Project(project)) | Row::Archive(project) => Some(*project),
        Row::Entry { project, .. } => Some(*project),
        Row::Group(Group::Space(_) | Group::Agent(_)) | Row::Heading(_) | Row::App(_) => None,
    }
}

/// Whether the kind a row names is switched on. Articles and apps have no switch, and
/// neither do the two rows that are not entries — a project heading and the
/// line its archive folds under stand whatever is listed beneath them.
pub(crate) fn shown(row: &Row, features: &Features) -> bool {
    match row.showing() {
        Some(Showing::Session(_)) => features.sessions,
        Some(Showing::Board(_)) => features.boards,
        Some(Showing::Table(_)) => features.tables,
        Some(Showing::Article(_)) | None => true,
    }
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

/// What the sidebar's list and the window's strip carry, and what a pane takes
/// a drop of: a sidebar row, or a tab off the window's strip.
#[derive(Clone, PartialEq)]
pub(crate) enum Dragged {
    Row(Row),
    /// A tab of the window's strip.
    Tab(crate::model::workspace::Slot),
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

/// Where each of `rows` starts down the list, and where the last one ends.
fn tops(shapes: &[Shape]) -> Vec<f32> {
    std::iter::once(0.)
        .chain(shapes.iter().scan(0., |at, shape| {
            *at += shape.pitch();
            Some(*at)
        }))
        .collect()
}

/// The list offset `y` down a list whose rows start at `tops`.
fn offset_at(tops: &[f32], y: f32) -> ListOffset {
    let ix = tops.iter().rposition(|top| *top <= y).unwrap_or(0);
    ListOffset {
        item_ix: ix,
        offset_in_item: px(y - tops[ix]),
    }
}

/// What rides under the pointer while an item is carried to a pane.
pub(crate) fn ghost(label: SharedString, theme: &Theme) -> AnyElement {
    div()
        .rounded(px(Theme::control_radius()))
        .px(px(10.))
        .py(px(4.))
        .text_style(TextStyle::Callout)
        .text_color(theme.text)
        .truncate()
        .child(label)
        .ghost(theme)
        .into_any_element()
}

/// An entry's own name in the element tree: two rows must never share one.
pub(crate) fn key_of(entry: &Row) -> String {
    match entry {
        Row::Group(Group::Project(project)) => format!("project-{project}"),
        Row::Group(Group::Space(id)) => format!("space-{id}"),
        Row::Group(Group::Agent(id)) => format!("agent-{id}"),
        Row::Heading(Heading::Projects) => "projects".to_owned(),
        Row::Heading(Heading::Agents) => "agents".to_owned(),
        Row::Heading(Heading::Spaces) => "spaces".to_owned(),
        Row::Heading(Heading::Apps) => "apps".to_owned(),
        Row::Archive(project) => format!("archive-{project}"),
        Row::Entry { project, showing } => match showing {
            Showing::Session(id) => format!("session-{project}-{id}"),
            Showing::Board(id) => format!("board-{project}-{id}"),
            Showing::Article(id) => format!("article-{project}-{id}"),
            Showing::Table(key) => format!("table-{project}-{key}"),
        },
        Row::App(id) => format!("app-{id}"),
    }
}

pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    selected: Selection,
    lifted: bool,
    indent: u8,
    trail: f32,
    theme: &Theme,
) -> Stateful<Div> {
    let fill = selected.fill(theme).filter(|_| !lifted);
    row_frame(id, group, indent, trail)
        // A carried row is drawn inside [`Sidebar::sidebar_ghost`]'s frame.
        .when(lifted, |el| el.m_0())
        .when_some(fill, |el, fill| el.bg(fill).shadow(selected.shadows()))
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!lifted && !selected.is_selected(), |el| {
            el.hover(|el| el.bg(theme.element_hover))
        })
}

/// A row's place in the column with none of its washes: what a line that is
/// never selected and takes no hover wash — a section's heading — stands in.
/// `trail` is what it stands off the column's right edge by — see
/// [`Sidebar::trail`].
pub(crate) fn row_frame(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    indent: u8,
    trail: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .h(px(ROW_PILL))
        .ml(px(root::SIDEBAR_GUTTER))
        .mr(px(trail))
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

/// A row's name. The line height is what the field pins itself to: left to
/// gpui's default the label's box is φ×13, and renaming would resize the row
/// under the name being typed.
fn row_label(name: String, tint: Hsla) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_style(TextStyle::Body)
        .line_height(px(18.))
        .text_color(tint)
        .child(name)
        .into_any_element()
}

/// The rows a sidebar menu opens with, built when it opens; they act on the
/// window.
type MenuRows = Rc<dyn Fn(&mut Sidebar, &mut Context<Sidebar>) -> Vec<(Item, Act)>>;

/// Which of the sidebar's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SidebarMenu {
    /// The `+` on a project heading: what to start here.
    Add(ProjectId),
    /// A project heading: what to do to the project.
    Project(ProjectId),
    /// The `···` on an entry's row, whichever kind it is, and a space's.
    Entry(Row),
    /// An agent identity's heading, by its id.
    Agent(String),
}

/// The column under the window's band: the search and library rows, the list
/// of projects and spaces, and the controls at its foot. Drawn cached; it
/// redraws when the workspace changes what it lists, when a session moves
/// what its row shows, and when the window hands it a new [`Lit`].
pub(crate) struct Sidebar {
    workspace: Entity<Workspace>,
    root: WeakEntity<Cydonia>,
    search: Entity<Search>,
    lit: Lit,
    menu: ContextMenu<Self, SidebarMenu>,
    /// The row or heading under the pointer, as the menu it would open.
    hovered: Option<SidebarMenu>,
    rename: Rename,
    /// The drag domain of the list: a row moves within it, or out onto a pane.
    pub(crate) sort: drag::Domain<(), Dragged>,
    rail: ListState,
    /// The list as [`Self::relist`] last worked it out. Render reads it and
    /// never works it out.
    listing: Listing,
    /// The rows `rail` holds measurements for, with how each was drawn. A row
    /// whose shape changes is spliced out and measured again.
    measured: Vec<(Row, Shape)>,
    /// Redraws the extended rows every [`TICK`] while they are on.
    tick: Option<Task<()>>,
    boards: Watch,
    /// Every app's program, whose build names its row.
    apps: Watch,
    /// What each session's row showed when it was last drawn.
    listed: HashMap<u64, Listed>,
    sessions: Watch,
    focus: FocusHandle,
    /// Whether [`Self::focus`] holds the keyboard focus, as of the frame being
    /// drawn: its selected row is emphasized while it does.
    emphasized: bool,
    _focus_watch: FocusWatch,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        search: Entity<Search>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let focus_watch = FocusWatch::new(&focus, window, cx);
        let rename = Rename::new(workspace.clone(), |this: &mut Self| &mut this.rename, cx);
        #[allow(unused_mut)]
        let mut subscriptions = vec![
            cx.observe(&workspace, |this, _, cx| this.relisted(cx)),
            cx.observe(&search, |this, _, cx| this.relisted(cx)),
        ];
        // The notice at its foot is the updater's, and the updater moves on its
        // own clock — and from the other window, where the Developer switch
        // that previews it lives.
        #[cfg(feature = "desktop")]
        if let Some(updater) = crate::model::update::of(cx) {
            subscriptions.push(cx.observe(&updater, |_, _, cx| cx.notify()));
        }
        let sort = drag::Domain::with_ghost({
            let this = cx.entity().downgrade();
            move |item, window, cx| {
                this.update(cx, |this, cx| this.sidebar_ghost(item, window, cx))
                    .unwrap_or_else(|_| Empty.into_any_element())
            }
        });
        let mut this = Self {
            workspace,
            root,
            search,
            lit: Lit::default(),
            menu: ContextMenu::default(),
            hovered: None,
            rename,
            sort,
            rail: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            listing: Listing::default(),
            measured: Vec::new(),
            tick: None,
            boards: Watch::default(),
            apps: Watch::default(),
            listed: HashMap::new(),
            sessions: Watch::default(),
            focus,
            emphasized: false,
            _focus_watch: focus_watch,
            _subscriptions: subscriptions,
        };
        this.relist(cx);
        this
    }

    /// Work the list out again from the workspace and the applied search.
    fn relist(&mut self, cx: &App) {
        let list = self.list(cx);
        let rows = list.rows();
        self.listing = Listing {
            shapes: Rc::new(self.shapes(&rows, cx)),
            places: Rc::new(list.row_places(&rows)),
            narrowed: Rc::new(rows.iter().map(|row| list.narrowed(row)).collect()),
            rows: Rc::new(rows),
        };
    }

    /// [`Self::relist`], and redraw.
    fn relisted(&mut self, cx: &mut Context<Self>) {
        self.relist(cx);
        cx.notify();
    }

    /// What a row stands off the column's right edge by: the gutter, or
    /// nothing while floating, where the column already stands as far from
    /// the cards as two cards stand apart.
    fn trail(&self, cx: &App) -> f32 {
        match self.workspace.read(cx).settings.appearance.floating {
            true => 0.,
            false => root::SIDEBAR_GUTTER,
        }
    }

    fn list<'a>(&self, cx: &'a App) -> List<'a> {
        List::new(&self.workspace, &self.search, cx)
    }

    /// The listing drawn: the list's, less the rows of a project or a space
    /// whose heading is being carried, down to the next heading.
    fn drawn(&self) -> Listing {
        let listing = &self.listing;
        let rows = &listing.rows;
        let Some(Dragged::Row(head @ Row::Group(_))) = self.sort.carried() else {
            return listing.clone();
        };
        let Some(at) = rows.iter().position(|row| *row == head) else {
            return listing.clone();
        };
        let end = rows[at + 1..]
            .iter()
            .position(|row| matches!(row, Row::Heading(_) | Row::Group(_)))
            .map_or(rows.len(), |after| at + 1 + after);
        fn cut<T: Clone>(all: &[T], gone: std::ops::Range<usize>) -> Rc<Vec<T>> {
            Rc::new([&all[..gone.start], &all[gone.end..]].concat())
        }
        Listing {
            rows: cut(rows, at + 1..end),
            shapes: cut(&listing.shapes, at + 1..end),
            places: listing.places.clone(),
            narrowed: cut(&listing.narrowed, at + 1..end),
        }
    }

    /// What the window is showing, handed over as it changes.
    pub(crate) fn set_lit(&mut self, lit: Lit, cx: &mut Context<Self>) {
        if self.lit != lit {
            self.lit = lit;
            cx.notify();
        }
    }

    /// Observe every session, redrawing where what its row shows moved.
    fn watch_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chats = self.workspace.read(cx).chat_entities();
        let mut sessions = std::mem::take(&mut self.sessions);
        sessions.set_in(&chats, window, cx, |this, chat, _, cx| {
            let extended = this.extended(cx);
            let listed = Listed::of(chat.read(cx), extended);
            if this.listed.insert(chat.read(cx).id, listed.clone()) != Some(listed) {
                this.relisted(cx);
            }
        });
        self.sessions = sessions;
    }

    /// Whether the entries are drawn extended — see
    /// [`crate::model::settings::Appearance::extended_sidebar`].
    fn extended(&self, cx: &App) -> bool {
        self.workspace.read(cx).settings.appearance.extended_sidebar
    }

    /// How `row` is drawn.
    fn shape(&self, row: &Row, cx: &App) -> Shape {
        let Row::Entry { showing, .. } = row else {
            return Shape::Line;
        };
        if !self.extended(cx) {
            return Shape::Line;
        }
        let workspace = self.workspace.read(cx);
        let labelled = row
            .located(workspace)
            .and_then(|(project, _)| workspace.labels_of(project, showing, cx))
            .is_some_and(|labels| !labels.is_empty());
        Shape::Extended { labelled }
    }

    fn shapes(&self, rows: &[Row], cx: &App) -> Vec<Shape> {
        rows.iter().map(|row| self.shape(row, cx)).collect()
    }

    /// Observe every board, whose description an extended row previews.
    fn watch_boards(&mut self, cx: &mut Context<Self>) {
        let boards: Vec<Entity<artifact::board::Board>> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|project| &project.boards)
            .map(|held| held.board.clone())
            .collect();
        let mut watch = std::mem::take(&mut self.boards);
        watch.set(&boards, cx);
        self.boards = watch;
    }

    /// Keep [`Self::tick`] running exactly while the rows are extended.
    fn keep_time(&mut self, extended: bool, cx: &mut Context<Self>) {
        match (extended, self.tick.is_some()) {
            (true, false) => {
                self.tick = Some(cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(TICK).await;
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            return;
                        }
                    }
                }));
            }
            (false, true) => self.tick = None,
            _ => {}
        }
    }

    /// Bring the list's measurements in line with `rows`: the run between the
    /// longest unchanged head and tail is spliced, so the rows outside it keep
    /// their heights and the scroll position they anchor.
    fn sync(&mut self, rows: &[Row], shapes: &[Shape]) {
        let rows: Vec<(Row, Shape)> = rows.iter().cloned().zip(shapes.iter().copied()).collect();
        if self.measured == rows {
            return;
        }
        let held = &self.measured;
        let head = held.iter().zip(&rows).take_while(|(a, b)| a == b).count();
        let tail = held[head..]
            .iter()
            .rev()
            .zip(rows[head..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        self.rail
            .splice(head..held.len() - tail, rows.len() - tail - head);
        self.measured = rows;
    }

    fn hover(&mut self, menu: SidebarMenu, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered.as_ref() == Some(&menu) {
                return;
            }
            self.hovered = Some(menu);
        } else if self.hovered.as_ref() == Some(&menu) {
            self.hovered = None;
        } else {
            return;
        }
        cx.notify();
    }

    /// Open `menu` at `place` with the rows `rows` builds, or shut it where it
    /// is the one up. The rows act on the window.
    fn menu_open(&self, menu: &SidebarMenu) -> bool {
        self.menu.key() == Some(menu)
    }

    /// A `···` or `+` that opens `menu` below itself. `group` is the hover
    /// group that reveals it; `None` for one always on show.
    #[allow(clippy::too_many_arguments)]
    fn menu_button(
        &self,
        id: impl Into<SharedString>,
        group: Option<&'static str>,
        mark: impl Into<Icon>,
        menu: SidebarMenu,
        place: MenuPlace,
        rows: MenuRows,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        // Resolved during render, never in a hover style: GPUI can resolve
        // hover differently in prepaint and paint.
        let reveal = match (group, &menu) {
            (None, _) => menu::Reveal::Always,
            (Some(_), SidebarMenu::Entry(_)) => {
                menu::Reveal::Shown(self.hovered.as_ref() == Some(&menu))
            }
            // The project's head reports its hover as its `···`.
            (Some(_), SidebarMenu::Add(path)) => menu::Reveal::Shown(
                matches!(&self.hovered, Some(SidebarMenu::Project(at)) if at == path),
            ),
            (Some(group), _) => menu::Reveal::Group(group),
        };
        let button = menu::button(id, mark, reveal, self.menu_open(&menu), cx).on_click({
            let menu = menu.clone();
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                let rows = rows.clone();
                this.toggle_menu(
                    menu.clone(),
                    place,
                    move |this, _, cx| Self::through(rows(this, cx)),
                    window,
                    cx,
                );
            })
        });
        self.menu_trigger(button, menu, cx)
    }

    /// The rows of an entry's menu, as the window builds them for a row here.
    fn entry_rows(&self, entry: &Row, cx: &App) -> Vec<(Item, Act)> {
        let archived = entry.archived(self.workspace.read(cx), cx);
        self.root
            .upgrade()
            .map(|root| {
                root.read(cx).entry_rows(
                    crate::view::window::entry::Host::Sidebar,
                    entry,
                    archived,
                    cx,
                )
            })
            .unwrap_or_default()
    }

    /// Put the name field on `what`'s row.
    pub(crate) fn start_rename(
        &mut self,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rename.start(what, window, cx);
        cx.notify();
    }

    /// Scroll the rail to a row, if it is not already on screen: by as little
    /// as shows it, so a step to the neighbour below moves the list by a row
    /// rather than throwing the one you came from off the top of it.
    pub(crate) fn reveal(&mut self, row: &Row, cx: &mut Context<Self>) {
        // Called straight after the change that put the row in the list,
        // before the workspace's observers have run.
        self.relist(cx);
        let Listing { rows, shapes, .. } = self.listing.clone();
        let Some(ix) = rows.iter().position(|at| at == row) else {
            return;
        };
        self.sync(&rows, &shapes);
        let tops = tops(&shapes);
        let scroll = self.scrolled(&tops);
        let height = f32::from(self.rail.viewport_bounds().size.height);
        let (top, bottom) = (tops[ix], tops[ix + 1]);
        let to = match () {
            _ if top < scroll => top,
            _ if bottom > scroll + height => bottom - height,
            _ => return,
        };
        self.rail.scroll_to(offset_at(&tops, to));
    }

    /// Take the list back to where a group starts, heading and all.
    fn scroll_to_group(&mut self, group: &Group, cx: &mut Context<Self>) {
        self.relist(cx);
        let Listing { rows, shapes, .. } = self.listing.clone();
        let Some(at) = rows
            .iter()
            .position(|row| matches!(row, Row::Group(at) if at == group))
        else {
            return;
        };
        self.sync(&rows, &shapes);
        self.rail.scroll_to(ListOffset {
            item_ix: at,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    /// How far down the list is scrolled, over rows whose tops are `tops`.
    fn scrolled(&self, tops: &[f32]) -> f32 {
        let at = self.rail.logical_scroll_top();
        let last = tops.len().saturating_sub(1);
        tops.get(at.item_ix.min(last)).copied().unwrap_or(0.) + f32::from(at.offset_in_item)
    }

    /// Fold a group's rows away, or bring them back.
    fn fold_group(&mut self, group: &Group, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.read(cx).query().is_some() {
            return;
        }
        self.defer_window(window, cx, |root, _, cx| root.commit(cx));
        self.workspace.update(cx, |workspace, cx| match group {
            Group::Project(path) => {
                if let Some(ix) = workspace.project_ix(*path) {
                    workspace.toggle_project(ix, cx);
                }
            }
            Group::Space(id) => workspace.toggle_space(id, cx),
            Group::Agent(id) => workspace.toggle_agent(id, cx),
        });
    }

    fn toggle_archive(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.defer_window(window, cx, |root, _, cx| root.commit(cx));
        self.workspace
            .update(cx, |workspace, cx| workspace.toggle_archive(project, cx));
    }

    /// The rows as one drag region over the list: an entry moves within the
    /// region of its project it is in, a project's or a space's heading among
    /// the headings of its kind, and either can be carried out onto a pane.
    fn sidebar_list(&mut self, drawn: Listing, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Listing {
            rows,
            shapes,
            places,
            narrowed,
        } = drawn;
        self.sync(&rows, &shapes);
        let view = cx.entity().downgrade();
        let list = gpui::list(self.rail.clone(), move |ix, window, cx| {
            let Some(row) = rows.get(ix).cloned() else {
                return Empty.into_any_element();
            };
            view.update(cx, |this, cx| {
                let item = Dragged::Row(row.clone());
                let lifted = this.sort.carried().as_ref() == Some(&item);
                let el = this.sidebar_row(&row, lifted, window, cx);
                match row {
                    Row::Heading(_) | Row::Archive(_) => {
                        this.sort.fixed(item, el).into_any_element()
                    }
                    _ if narrowed[ix] => this.sort.fixed(item, el).into_any_element(),
                    _ => this.sort.handle(item, el).into_any_element(),
                }
            })
            .unwrap_or_else(|_| Empty.into_any_element())
        })
        .size_full();
        self.sort
            .region("sidebar-list", (), gpui::Axis::Vertical, list)
            .track_scroll(&self.rail)
            .accepts(|item| matches!(item, Dragged::Row(_)))
            .lands(move |item, after, before| lands(&places, item, after, before))
            .size_full()
            .on_drop(cx.listener(|this, event: &drag::Drop<(), Dragged>, _, cx| {
                this.sidebar_moved(event, cx)
            }))
    }

    /// The heading of the group whose rows are under the scroll, held at the
    /// top of the list while they pass beneath it, and pushed out by the next
    /// heading rather than sliding under it. Nothing while the heading's own
    /// row is on screen.
    fn pinned_head(
        &self,
        rows: &[Row],
        shapes: &[Shape],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let head = |row: &Row| matches!(row, Row::Group(_));
        let tops = tops(shapes);
        let scroll = self.scrolled(&tops);
        let first = self
            .rail
            .logical_scroll_top()
            .item_ix
            .min(rows.len().checked_sub(1)?);
        let at = rows[..=first].iter().rposition(head)?;
        let Row::Group(group) = rows[at].clone() else {
            return None;
        };
        let limit = rows[at + 1..]
            .iter()
            .position(head)
            .map(|after| tops[at + 1 + after] - ROW_HEIGHT);
        let held = limit.map_or(scroll, |limit| scroll.min(limit));
        if held <= tops[at] {
            return None;
        }
        let y = held - scroll;
        Some(
            // Stateful, and so an id scope of its own: the copy inside
            // carries the same ids as the row it stands for.
            //
            // The band the glass is seen in, and what clips it to one row.
            div()
                .id("pinned-head")
                .absolute()
                .top(px(y))
                .left_0()
                .w_full()
                .h(px(ROW_HEIGHT))
                .overflow_hidden()
                .child(self.group_head(group, true, false, window, cx))
                .into_any_element(),
        )
    }

    /// What rides under the pointer while a sidebar row is carried: the row
    /// itself, at its width in the list.
    fn sidebar_ghost(
        &self,
        item: &Dragged,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Dragged::Row(row) = item else {
            return Empty.into_any_element();
        };
        let theme = Theme::of(cx).clone();
        let width = self
            .root
            .upgrade()
            .map_or(root::SIDEBAR_WIDTH, |root| root.read(cx).sidebar_width);
        div()
            .w(px(width - root::SIDEBAR_GUTTER - self.trail(cx)))
            .h(px(self.shape(row, cx).pill()))
            .rounded(px(Theme::control_radius()))
            .overflow_hidden()
            .child(self.sidebar_row(row, true, window, cx))
            .ghost(&theme)
            .into_any_element()
    }

    /// A row let go in the list, between the painted rows `after` and
    /// `before`.
    fn sidebar_moved(&mut self, event: &drag::Drop<(), Dragged>, cx: &mut Context<Self>) {
        let Dragged::Row(carried) = &event.item else {
            return;
        };
        let rows = self.list(cx).rows();
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
            Row::Group(Group::Project(project)) => {
                let to = above
                    .filter(|row| matches!(row, Row::Group(Group::Project(_))))
                    .count();
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_project(*project, to, cx));
            }
            Row::Group(Group::Space(id)) => {
                let to = above
                    .filter(|row| matches!(row, Row::Group(Group::Space(_))))
                    .count();
                self.workspace
                    .update(cx, |workspace, cx| workspace.move_space(id, to, cx));
            }
            _ => {
                let places = self.list(cx).row_places(&rows);
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
                    // An identity's sessions run by when they were touched.
                    Some(Group::Agent(_)) => {}
                    // The sidebar's order alone: the space's panes stay put.
                    Some(Group::Space(id)) => {
                        let workspace = self.workspace.read(cx);
                        let members: Vec<Member> = region
                            .iter()
                            .filter_map(|row| row.member(workspace, cx))
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
    /// button at the end of the row and the title row's `···` are for.
    fn reorder_entries(&mut self, carried: &Row, region: Vec<Row>, cx: &mut Context<Self>) {
        use crate::model::state;
        let workspace = self.workspace.read(cx);
        let Some(project) = project_of(carried).and_then(|project| workspace.project_ix(project))
        else {
            return;
        };
        let rows: Vec<Row> = self
            .list(cx)
            .ranked(project)
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
        let among_pins = carried.pinned(workspace, cx);
        let entry_of = |row: &Row| workspace.entry_of(project, row.showing()?.clone(), cx);
        // Every entry, so that unpinning one later puts it back where it sat
        // rather than at the top.
        let order: Vec<state::Entry> = moved.iter().filter_map(entry_of).collect();
        // And the pins again, when it was one of them that moved: their own
        // order is what the region above is listed by.
        let pins: Option<Vec<state::Entry>> = among_pins.then(|| {
            moved
                .iter()
                .filter(|row| row.pinned(workspace, cx))
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
}

impl MenuHost for Sidebar {
    type Key = SidebarMenu;

    fn menu(&self) -> &ContextMenu<Self, SidebarMenu> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, SidebarMenu> {
        &mut self.menu
    }
}

impl InWindow for Sidebar {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.watch_sessions(window, cx);
        self.emphasized = self.focus.contains_focused(window, cx);
        let programs: Vec<_> = self.workspace.read(cx).programs().cloned().collect();
        let mut apps = std::mem::take(&mut self.apps);
        // A build names an app's row, and the apps are listed by name.
        apps.set_in(&programs, window, cx, |this, _, _, cx| this.relisted(cx));
        self.apps = apps;
        let extended = self.extended(cx);
        if extended {
            self.watch_boards(cx);
        }
        self.keep_time(extended, cx);
        let drawn = self.drawn();
        let pinned = self.pinned_head(&drawn.rows, &drawn.shapes, window, cx);
        let visibility = self
            .workspace
            .read(cx)
            .settings
            .appearance
            .sidebar_scrollbars;
        div()
            .track_focus(&self.focus)
            // A press on a row that opens an entry moves the focus on into
            // its pane.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.focus.contains_focused(window, cx) {
                        window.focus(&this.focus, cx);
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .child(self.search_row(cx))
            .child(self.library_entry(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.sidebar_list(drawn, cx))
                    .children(pinned)
                    .child(
                        scrollbars::Overlay::new(
                            "sidebar-bar",
                            &self.rail,
                            bezel::gpui::Axis::Vertical,
                        )
                        .visibility(visibility.into()),
                    ),
            )
            .children(self.restart_notice(cx))
            .child(self.settings_row(cx))
    }
}
