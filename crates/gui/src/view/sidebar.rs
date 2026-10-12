//! The projects sidebar: a folding heading per project, and every session,
//! article and table in it, drawn by its own [`Sidebar`] entity under the band
//! the window draws at the top of the column.

mod list;
mod rows;

pub(crate) use list::List;

use crate::model::{
    session::ChatSession,
    settings::Features,
    workspace::{Showing, Workspace},
};
use crate::view::{
    kit::{
        menu::{self, Act},
        rename::{NameEvent, NameField},
        watch::Watch,
    },
    search::Search,
    window::{reach::InWindow, root::Cydonia},
};
use artifact::kind::Kind;
use artifact::space::Member;
use bezel::ui::scroll as scrollbars;
use bezel::{
    agent::orbs::{OrbState, engine::Frame},
    gpui::{
        self, AnyElement, App, AppContext as _, Bounds, Context, Div, Empty, Entity, Hsla, Pixels,
        Point, ScrollStrategy, SharedString, Stateful, Subscription, UniformListDecoration,
        UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
    },
    motion::Painter,
    theme::{self, TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place as MenuPlace},
        drag::{self, Carried},
        icons::{self, Icon},
        input::Case,
        menu::Item,
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
    /// The turn in flight, as the orb needs it: which of the twelve, how long
    /// it has been running, and the buffer it paints into. `None` when nothing
    /// is in flight, which is what puts the agent's own mark back.
    working: Option<Working>,
}

/// A session's orb, read off the model with the row.
struct Working {
    state: OrbState,
    since: Duration,
    frame: Rc<RefCell<Frame>>,
}

/// What the sidebar shows of a session — what moving redraws it.
#[derive(Clone, PartialEq)]
struct Listed {
    label: String,
    streaming: bool,
    asking: bool,
    closed: bool,
    labels: Vec<String>,
    number: Option<u64>,
}

impl Listed {
    fn of(chat: &ChatSession) -> Self {
        Self {
            label: chat.label(),
            streaming: chat.streaming,
            asking: chat.permission.is_some(),
            closed: chat.closed,
            labels: chat.labels.clone(),
            number: chat.number,
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
    /// The line the archived entries of the project at this path are folded
    /// under.
    Archive(PathBuf),
    /// An entry of the project at `project`.
    Entry { project: PathBuf, showing: Showing },
}

impl Row {
    /// The kind of entry it is. Nothing for a row that is not an entry.
    pub(crate) fn kind(&self) -> Option<Kind> {
        match self {
            Row::Entry { showing, .. } => Some(showing.kind()),
            _ => None,
        }
    }

    /// What an entry row stands for, and `None` for the rows that are not
    /// entries.
    pub(crate) fn showing(&self) -> Option<&Showing> {
        match self {
            Row::Entry { showing, .. } => Some(showing),
            Row::Group(_) | Row::Archive(_) | Row::Heading(_) => None,
        }
    }

    /// Where an entry row's project is listed now, and where its entry is
    /// listed among its kind in that project.
    pub(crate) fn located(&self, workspace: &Workspace) -> Option<(usize, usize)> {
        let Row::Entry { project, showing } = self else {
            return None;
        };
        let at = workspace.project_at(project)?;
        Some((at, workspace.projects[at].ix_of(showing)?))
    }

    /// What a space would name this row, so it can be dragged into one.
    pub(crate) fn member(&self, workspace: &Workspace, cx: &App) -> Option<Member> {
        let Row::Entry { project, showing } = self else {
            return None;
        };
        workspace.member_of(workspace.project_at(project)?, showing.clone(), cx)
    }

    /// The row for a member a space holds — the way back from what it names
    /// to the line that stands for it. Nothing where its project is not open.
    pub(crate) fn of_member(member: &Member, workspace: &Workspace, cx: &App) -> Option<Self> {
        let (_, showing) = workspace.showing_of(member, cx)?;
        Some(Row::Entry {
            project: member.project.clone(),
            showing,
        })
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
            workspace.member_of(workspace.project_at(project)?, Showing::Session(id), cx)
        })
    }

    /// What the row is called.
    pub(crate) fn label(&self, workspace: &Workspace, cx: &App) -> String {
        let named = || -> Option<String> {
            Some(match self {
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
            .project_at(project)
            .is_some_and(|project| workspace.is_pinned(project, showing.clone(), cx))
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
/// [`Sidebar::group_head`].
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

/// What the window is showing, as a row's [`Light`] is read against. The
/// window hands it to the sidebar as it changes.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Lit {
    /// The library stands over every pane.
    pub(crate) library: bool,
    /// The focused pane's entry, in a space.
    pub(crate) focused: Option<Member>,
    /// The kind the pane in front shows.
    pub(crate) showing: Option<Kind>,
}

impl Lit {
    /// Whether the pane in front is on this entry. Never while the library
    /// stands over the panes.
    pub(crate) fn in_front(&self, entry: &Row, workspace: &Workspace) -> bool {
        let Row::Entry { showing, .. } = entry else {
            return false;
        };
        if self.library {
            return false;
        }
        let Some((project, ix)) = entry.located(workspace) else {
            return false;
        };
        if workspace.active != Some(project) || workspace.active_space().is_some() {
            return false;
        }
        let open = &workspace.projects[project];
        let pane = self.showing;
        match showing {
            Showing::Session(id) => pane == Some(Kind::Session) && open.active == Some(*id),
            Showing::Board(_) => pane == Some(Kind::Board) && open.board == Some(ix),
            Showing::Article(_) => pane == Some(Kind::Article) && open.article == Some(ix),
            Showing::Table(_) => pane == Some(Kind::Table) && open.table == Some(ix),
        }
    }

    /// How a row's entry stands to what the window is showing — see
    /// [`Light`]. In a space, the focused pane's entry is the lit one and the
    /// others it shows are [`Light::Shown`]; on one entry alone, that entry.
    pub(crate) fn light(&self, row: &Row, workspace: &Workspace, cx: &App) -> Light {
        // The library stands over every pane, so no row is what is on screen.
        if self.library {
            return Light::Off;
        }
        let Some(space) = workspace.active_space() else {
            return match self.in_front(row, workspace) {
                true => Light::Focused,
                false => Light::Off,
            };
        };
        let Some(member) = row.member(workspace, cx) else {
            return Light::Off;
        };
        if self.focused.as_ref() == Some(&member) {
            Light::Focused
        } else if space.entries().contains(&member) {
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
pub(crate) fn shown(row: &Row, features: &Features) -> bool {
    match row.showing() {
        Some(Showing::Session(_)) => features.sessions,
        Some(Showing::Board(_)) => features.boards,
        Some(Showing::Table(_)) => features.tables,
        Some(Showing::Article(_)) | None => true,
    }
}

/// What a name field is attached to. Each entry is held by what identifies it
/// — a session, a table's key — never by an index: that moves the moment a
/// neighbour is made or dropped, and the field would follow it onto whichever
/// entry slid underneath.
///
/// No article here: its title is the first line of its own page, which is
/// where it is written — see [`crate::view::article`]. No board either: a
/// board is named by its identity panel — see [`Cydonia::open_info`].
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Renaming {
    Session(u64),
    Table(String),
    /// A space, by its id — auto-named `space-1` until someone gives it a
    /// name of their own.
    Space(String),
}

impl Renaming {
    /// How `row` is renamed in a field, for the kinds named that way.
    pub(crate) fn of(row: &Row) -> Option<Self> {
        match row {
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => Some(Self::Session(*id)),
            Row::Entry {
                showing: Showing::Table(key),
                ..
            } => Some(Self::Table(key.clone())),
            Row::Group(Group::Space(id)) => Some(Self::Space(id.clone())),
            Row::Entry {
                showing: Showing::Article(_) | Showing::Board(_),
                ..
            }
            | Row::Group(Group::Project(_))
            | Row::Archive(_)
            | Row::Heading(_) => None,
        }
    }

    /// The name it has now, which the field opens holding.
    pub(crate) fn label(&self, workspace: &Workspace, cx: &App) -> String {
        match self {
            Self::Session(id) => workspace
                .session(*id, cx)
                .map(ChatSession::label)
                .unwrap_or_default(),
            Self::Table(key) => workspace
                .projects
                .iter()
                .flat_map(|open| open.tables.iter())
                .find(|table| table.key == *key)
                .map(|table| table.name.clone())
                .unwrap_or_default(),
            Self::Space(id) => workspace
                .spaces
                .iter()
                .find(|space| space.id == *id)
                .map(|space| space.name.clone())
                .unwrap_or_default(),
        }
    }

    /// Give it `name`.
    pub(crate) fn commit(
        self,
        name: String,
        workspace: &mut Workspace,
        cx: &mut Context<Workspace>,
    ) {
        match self {
            Self::Session(id) => workspace.rename_session(id, name, cx),
            Self::Table(key) => workspace.rename_table(&key, name, cx),
            Self::Space(id) => workspace.rename_space(&id, name, cx),
        }
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

pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    selected: bool,
    lifted: bool,
    indent: u8,
    theme: &Theme,
) -> Stateful<Div> {
    row_frame(id, group, indent)
        // A carried row is drawn inside [`Sidebar::sidebar_ghost`]'s frame.
        .when(lifted, |el| el.m_0())
        .when(!lifted && selected, |el| {
            el.bg(theme.card_selected_bg())
                .shadow(theme::glass_selected_shadows())
        })
        // Only off the open row: the hover wash is the weaker rung, and
        // painting it over the selection would dim what the pointer is on.
        .when(!lifted && !selected, |el| {
            el.hover(|el| el.bg(theme.element_hover))
        })
}

/// A row's place in the column with none of its washes: what a line that is
/// never selected and takes no hover wash — a section's heading — stands in.
pub(crate) fn row_frame(
    id: impl Into<gpui::ElementId>,
    group: &'static str,
    indent: u8,
) -> Stateful<Div> {
    use crate::view::window::root;
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
struct PinnedHead(Entity<Sidebar>);

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

/// The rows a sidebar menu opens with, built when it opens; they act on the
/// window.
type MenuRows = Rc<dyn Fn(&mut Sidebar, &mut Context<Sidebar>) -> Vec<(Item, Act)>>;

/// Which of the sidebar's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SidebarMenu {
    /// The `+` on a project heading, by the project's path: what to start
    /// here.
    Add(PathBuf),
    /// A project heading, by the project's path: what to do to the project.
    Project(PathBuf),
    /// The `···` on an entry's row, whichever kind it is, and a space's.
    Entry(Row),
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
    renaming: Option<Renaming>,
    name_field: Entity<NameField>,
    /// The drag domain of the list: a row moves within it, or out onto a pane.
    pub(crate) sort: drag::Domain<(), Dragged>,
    rail: UniformListScrollHandle,
    /// What each session's row showed when it was last drawn.
    listed: HashMap<u64, Listed>,
    sessions: Watch,
    _subscriptions: Vec<Subscription>,
}

impl Sidebar {
    pub(crate) fn new(
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        search: Entity<Search>,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_field = cx.new(NameField::new);
        #[allow(unused_mut)]
        let mut subscriptions = vec![
            cx.subscribe(&name_field, |this, _, event: &NameEvent, cx| match event {
                NameEvent::Named(name) => this.commit_name(name.clone(), cx),
                NameEvent::Dismissed => this.dismiss_name(cx),
            }),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&search, |_, _, cx| cx.notify()),
        ];
        // The notice at its foot is the updater's, and the updater moves on its
        // own clock — and from the other window, where the Developer switch
        // that previews it lives.
        #[cfg(feature = "desktop")]
        if let Some(updater) = crate::model::update::of(cx) {
            subscriptions.push(cx.observe(&updater, |_, _, cx| cx.notify()));
        }
        let sort = drag::Domain::with_ghost(Painter::of(cx), {
            let this = cx.entity().downgrade();
            move |item, window, cx| {
                this.update(cx, |this, cx| this.sidebar_ghost(item, window, cx))
                    .unwrap_or_else(|_| Empty.into_any_element())
            }
        });
        Self {
            workspace,
            root,
            search,
            lit: Lit::default(),
            menu: ContextMenu::default(),
            hovered: None,
            renaming: None,
            name_field,
            sort,
            rail: UniformListScrollHandle::new(),
            listed: HashMap::new(),
            sessions: Watch::default(),
            _subscriptions: subscriptions,
        }
    }

    /// The list as it stands.
    fn list<'a>(&self, cx: &'a App) -> List<'a> {
        List::new(self.workspace.read(cx), self.search.read(cx), cx)
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
        let chats: Vec<Entity<ChatSession>> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|project| &project.sessions)
            .map(|held| held.chat.clone())
            .collect();
        let mut sessions = std::mem::take(&mut self.sessions);
        sessions.set_in(&chats, window, cx, |this, chat, _, cx| {
            let listed = Listed::of(chat.read(cx));
            if this.listed.insert(chat.read(cx).id, listed.clone()) != Some(listed) {
                cx.notify();
            }
        });
        self.sessions = sessions;
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
    fn toggle_menu(
        &mut self,
        menu: SidebarMenu,
        place: MenuPlace,
        rows: impl FnOnce(&mut Self, &mut Context<Self>) -> Vec<(Item, Act)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        ContextMenu::toggle(
            self,
            |this| &mut this.menu,
            menu,
            place,
            |this, _, cx| menu::chooser(Self::through(rows(this, cx))),
            window,
            cx,
        );
    }

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
                    move |this, cx| rows(this, cx),
                    window,
                    cx,
                );
            })
        });
        ContextMenu::trigger(button, |this: &mut Self| &mut this.menu, menu, cx)
    }

    /// The menu `menu`, placed, while it is on screen: what its trigger hangs
    /// beside itself.
    fn menu_here(
        &self,
        menu: &SidebarMenu,
        id: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.menu
            .render_for(menu, id, |this| &mut this.menu, window, cx)
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
        let label = what.label(self.workspace.read(cx), cx);
        self.name_field
            .update(cx, |field, cx| field.start(label, Case::Mixed, window, cx));
        self.renaming = Some(what);
        cx.notify();
    }

    fn commit_name(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| what.commit(name, workspace, cx));
        cx.notify();
    }

    fn dismiss_name(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    /// Scroll the rail to a row, if it is not already on screen. `Nearest`
    /// rather than `Top`: a step to the neighbour below should move the list by
    /// a row, not throw the one you came from off the top of it.
    pub(crate) fn reveal(&mut self, row: &Row, cx: &mut Context<Self>) {
        if let Some(ix) = self.list(cx).rows().iter().position(|at| at == row) {
            self.rail.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
    }

    /// Take the list back to where a group starts, heading and all.
    fn scroll_to_group(&mut self, group: &Group, cx: &mut Context<Self>) {
        let Some(at) = self
            .list(cx)
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Group(at) if at == group))
        else {
            return;
        };
        self.rail.scroll_to_item(at, ScrollStrategy::Top);
        cx.notify();
    }

    /// Fold a group's rows away, or bring them back.
    fn fold_group(&mut self, group: &Group, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.read(cx).query().is_some() {
            return;
        }
        self.defer_window(window, cx, |root, _, cx| root.commit(cx));
        self.workspace.update(cx, |workspace, cx| match group {
            Group::Project(path) => {
                if let Some(ix) = workspace.project_at(path) {
                    workspace.toggle_project(ix, cx);
                }
            }
            Group::Space(id) => workspace.toggle_space(id, cx),
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
    fn sidebar_list(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let list = self.list(cx);
        let places = Rc::new(list.row_places(&rows));
        let narrowed: Vec<bool> = rows.iter().map(|row| list.narrowed(row)).collect();
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
                    .filter(|item| this.sort.carries(item))
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
                                this.sort.fixed(item, el).into_any_element()
                            }
                            _ if narrowed[ix] => this.sort.fixed(item, el).into_any_element(),
                            _ => this.sort.handle(item, el).into_any_element(),
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.rail)
        .with_decoration(PinnedHead(cx.entity()))
        .size_full();
        self.sort
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
        let rows = self.list(cx).rows();
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

    /// What rides under the pointer while a sidebar row is carried: the row
    /// itself, at its width in the list.
    fn sidebar_ghost(
        &self,
        item: &Dragged,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::view::window::root;
        let Dragged::Row(row) = item else {
            return Empty.into_any_element();
        };
        let theme = Theme::of(cx).clone();
        let width = self
            .root
            .upgrade()
            .map_or(root::SIDEBAR_WIDTH, |root| root.read(cx).sidebar_width);
        div()
            .w(px(width - 2. * root::SIDEBAR_GUTTER))
            .h(px(ROW_PILL))
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
    /// button at the end of the row and the band's `···` are for.
    fn reorder_entries(&mut self, carried: &Row, region: Vec<Row>, cx: &mut Context<Self>) {
        use crate::model::state;
        let workspace = self.workspace.read(cx);
        let Some(project) = project_of(carried).and_then(|path| workspace.project_at(path)) else {
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

impl InWindow for Sidebar {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.watch_sessions(window, cx);
        let rows = self.list(cx).rows();
        let visibility = self
            .workspace
            .read(cx)
            .settings
            .appearance
            .sidebar_scrollbars;
        div()
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
                    .child(self.sidebar_list(rows, cx))
                    .child(
                        scrollbars::Overlay::new(
                            "sidebar-bar",
                            &self.rail.0.borrow().base_handle,
                            bezel::gpui::Axis::Vertical,
                        )
                        .visibility(visibility.into()),
                    ),
            )
            .children(self.restart_notice(cx))
            .child(self.settings_row(cx))
    }
}
