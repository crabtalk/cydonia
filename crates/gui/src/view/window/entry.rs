//! What the window does to an entry: open it, put it away, delete it, pin
//! it, name it, and the rows of its menu — whichever view's menu asked.

use crate::model::workspace::Showing;
use crate::view::{
    article::TogglePlainText,
    keymap::{self, Command},
    kit::{menu, stamp, text},
    sidebar::{Group, Lit, Renaming, Row, on_disk},
    window::root::Cydonia,
};
use artifact::board::{Page, View};
use artifact::space::Member;
use bezel::{
    gpui::{AnyElement, App, Context, Window, prelude::*},
    ui::{icons, input::Case, menu::Item},
};
use std::path::PathBuf;

/// Which view's menu an entry's rows are for.
#[derive(Clone, PartialEq)]
pub(crate) enum Host {
    /// A row in the sidebar.
    Sidebar,
    /// A row in the library.
    Library,
    /// The band over the window's one pane.
    Header,
    /// A tab of a space's pane, by the entry it is on.
    Tab(Member),
}

impl Cydonia {
    /// What the window is showing, as the sidebar's rows light against it.
    pub(crate) fn lit(&self, cx: &App) -> Lit {
        Lit {
            library: self.library.is_some(),
            focused: self.leaf().read(cx).entry.clone(),
            showing: self.showing(cx),
        }
    }

    /// The sidebar's list as it stands.
    pub(crate) fn list<'a>(&self, cx: &'a App) -> crate::view::sidebar::List<'a> {
        crate::view::sidebar::List::new(self.workspace.read(cx), self.search.read(cx), cx)
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
                    self.commit(cx);
                    self.workspace
                        .update(cx, |workspace, cx| workspace.toggle_archive(ix, cx));
                }
            }
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => self.select_session(*id, window, cx),
            Row::Entry { showing, .. } => {
                let Some((project, ix)) = row.located(self.workspace.read(cx)) else {
                    return;
                };
                match showing {
                    Showing::Board(_) => self.open_board(project, ix, window, cx),
                    Showing::Article(_) => self.open_article(project, ix, window, cx),
                    Showing::Table(_) => self.open_table(project, ix, window, cx),
                    Showing::App(_) => self.open_app(project, ix, window, cx),
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

    /// Show `path` in the file manager. Best effort and off the main thread:
    /// opening it is a process, and a file manager that will not come to the
    /// front is not worth blocking a frame over.
    pub(crate) fn reveal_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cfg!(not(feature = "desktop")) {
            self.desktop_only("Showing it in the file manager", window, cx);
            return;
        }
        crate::view::component::file::external::reveal(path, cx);
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

    /// Everything an entry can have done to it, for the menu `host` names:
    /// the band's `···`, a sidebar or library row, a tab. One builder for all
    /// of them, so a command reachable in one is reachable in every other.
    pub(crate) fn entry_rows(
        &self,
        host: Host,
        entry: &Row,
        archived: bool,
        cx: &App,
    ) -> Vec<(Item, menu::Act)> {
        // `../desktop`'s rule for what a `···` may carry: only commands with no
        // affordance on the object. An article's title is the head of its own
        // page and a board's name in the band opens its identity panel, so
        // neither is offered a second route here. Everywhere else the name is
        // display-only and this is the way.
        // An app is its crate: its name is its manifest's, and it is not
        // filed, pinned or deleted from here.
        let app = matches!(entry.showing(), Some(Showing::App(_)));
        let named = !matches!(
            entry.showing(),
            Some(Showing::Article(_) | Showing::Board(_) | Showing::App(_))
        );
        // A page's measure and how it is being read: the open page's, since
        // [`Self::set_full_width`] and [`Self::plain_text`] are about the one
        // the window is showing. A row's menu names an entry that may not be
        // it, so these are the band's alone — on the wrong row they would act
        // on whatever else was open.
        // A tab's menu has them only while its tab is the one focused.
        let page = match &host {
            Host::Sidebar | Host::Library => false,
            Host::Tab(tab) => self.leaf().read(cx).entry.as_ref() == Some(tab),
            Host::Header => true,
        };

        // ── how the open page is shown ──
        let mut shown: Vec<(Item, menu::Act)> = Vec::new();
        if matches!(entry.showing(), Some(Showing::Article(_))) && page {
            let plain_chord = keymap::label(
                Command::PlainText,
                &self.workspace.read(cx).settings.shortcuts,
            )
            .unwrap_or_default();
            // The markdown itself, for the times the document is in the way of
            // it. Above the width, which is about the page rather than what is
            // being edited on it.
            shown.push(menu::row::<Cydonia>(
                Item::action("Plain text")
                    .with_icon(icons::text::Code)
                    .with_keystroke(plain_chord)
                    .checked(self.plain_text(cx).unwrap_or_default()),
                move |this, window, cx| this.toggle_plain_text(&TogglePlainText, window, cx),
            ));
            // The page the focused pane is on, which is what these rows act on
            // — see [`Cydonia::pane_doc`].
            let held = self.pane_doc(cx).and_then(|article| article.full_width);
            let wide = held.unwrap_or(self.workspace.read(cx).wide_pages);
            shown.push(menu::row::<Cydonia>(
                Item::action("Full width")
                    .with_icon(icons::layout::UnfoldHorizontal)
                    .checked(wide),
                move |this, _, cx| this.set_full_width(Some(!wide), cx),
            ));
            // Only for a page carrying a measure of its own. On every other
            // page it is already what is happening, and a row that undoes
            // nothing is a row nobody can read the point of.
            if held.is_some() {
                shown.push(menu::row::<Cydonia>(
                    Item::action("Use default width").with_icon(icons::layout::Columns2),
                    move |this, _, cx| this.set_full_width(None, cx),
                ));
            }
            // Only where there is none. A page that has one is changed from
            // the picture itself, which is on screen and has nowhere else it
            // could mean — see `article::cover_controls`.
            if self
                .pane_doc(cx)
                .is_some_and(|article| article.cover.is_none())
            {
                shown.push(menu::row::<Cydonia>(
                    Item::action("Add cover").with_icon(icons::files::ImagePlus),
                    move |this, _, cx| this.shuffle_cover(cx),
                ));
            }
        }
        if let Some(Showing::Board(_)) = entry.showing()
            && !matches!(host, Host::Sidebar | Host::Library)
            && let Some((project, ix)) = entry.located(self.workspace.read(cx))
            && let Some((id, view, described)) = self
                .workspace
                .read(cx)
                .board_in(project, ix, cx)
                .map(|board| (board.id.clone(), board.view, !board.description.is_empty()))
        {
            if let Some(board) = self.workspace.read(cx).board_member(project, ix) {
                let on = match &host {
                    Host::Tab(tab) => Some(tab.clone()),
                    Host::Sidebar | Host::Library | Host::Header => None,
                };
                let label = match described {
                    true => "Description",
                    false => "Add description",
                };
                shown.push(menu::row::<Cydonia>(
                    Item::action(label).with_icon(icons::text::Book),
                    move |this, window, cx| {
                        this.open_page(on.as_ref(), board.clone(), Page::Description, window, cx)
                    },
                ));
            }
            shown.push(menu::switch::<Cydonia, _>(
                "View",
                icons::layout::LayoutList,
                vec![
                    (
                        View::Lanes,
                        icons::development::SquareKanban.into(),
                        "Lanes",
                    ),
                    (View::List, icons::layout::LayoutList.into(), "List"),
                ],
                view,
                move |this, view, _, cx| this.set_board_view(&id, view, cx),
            ));
        }

        if app && let Some(program) = entry.program(self.workspace.read(cx)) {
            shown.push(menu::row::<Cydonia>(
                Item::action("Build again").with_icon(icons::arrows::RefreshCw),
                move |_, _, cx| program.update(cx, |program, cx| program.rebuild(cx)),
            ));
        }

        // ── the entry itself ──
        let mut held: Vec<(Item, menu::Act)> = Vec::new();
        if named {
            held.push(menu::row::<Cydonia>(
                Item::action("Rename").with_icon(icons::text::SquarePen),
                {
                    let entry = entry.clone();
                    let host = host.clone();
                    move |this, window, cx| this.rename_entry(&entry, &host, window, cx)
                },
            ));
        }
        // Only for an entry still in hand: what is put away is not held at
        // the top of anything.
        if !archived && !app && entry.showing().is_some() {
            let pinned = entry.pinned(self.workspace.read(cx), cx);
            let pin = match pinned {
                true => Item::action("Unpin").with_icon(icons::navigation::PinOff),
                false => Item::action("Pin to top").with_icon(icons::navigation::Pin),
            };
            let entry = entry.clone();
            held.push(menu::row::<Cydonia>(pin, move |this, _, cx| {
                this.pin_entry(&entry, !pinned, cx)
            }));
        }
        let put = match archived {
            true => Item::action("Unarchive").with_icon(icons::files::ArchiveRestore),
            false => Item::action("Archive").with_icon(icons::files::Archive),
        };
        if !app {
            held.push(menu::row::<Cydonia>(put, {
                let entry = entry.clone();
                move |this, window, cx| this.archive_entry(&entry, !archived, window, cx)
            }));
        }
        // Into any other open project, the folder and its pictures with it.
        if let Some(Showing::Article(_)) = entry.showing()
            && let Some((project, ix)) = entry.located(self.workspace.read(cx))
        {
            let targets: Vec<(Item, menu::Act)> = self
                .workspace
                .read(cx)
                .projects
                .iter()
                .enumerate()
                .filter(|(at, _)| *at != project)
                .map(|(to, open)| {
                    menu::row::<Cydonia>(
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
                held.push(menu::submenu(
                    "Move to",
                    icons::arrows::ArrowRightLeft,
                    targets,
                ));
            }
        }

        // ── where it is on disk ──
        let disk: Vec<(Item, menu::Act)> = entry
            .place(self.workspace.read(cx), cx)
            .map(|path| on_disk(path).into_iter().collect())
            .unwrap_or_default();

        // ── the tab it is open in ──
        let mut tab_rows: Vec<(Item, menu::Act)> = Vec::new();
        if let Host::Tab(tab) = &host {
            let tab = tab.clone();
            tab_rows.push(menu::row::<Cydonia>(
                Item::action("Close tab").with_icon(icons::notifications::X),
                move |this, window, cx| this.close_pane(&tab, window, cx),
            ));
        }

        // Alone, so it is never the row beside the one meant.
        let mut delete = Vec::new();
        if !app {
            delete.push(menu::row::<Cydonia>(
                Item::action("Delete").with_icon(icons::files::Trash),
                {
                    let entry = entry.clone();
                    move |this, window, cx| this.ask_delete(&entry, window, cx)
                },
            ));
        }
        // ── what the open page is, at the foot: read, never chosen ──
        let mut about: Vec<(Item, menu::Act)> = Vec::new();
        if matches!(entry.showing(), Some(Showing::Article(_)))
            && page
            && let Some(article) = self.pane_doc(cx)
        {
            if let Some(words) = article.words() {
                about.push(menu::note(text::word_count(words)));
            }
            about.push(menu::note(format!(
                "Edited {}",
                stamp::full(article.touched)
            )));
            if let Some(created) = article.created {
                about.push(menu::note(format!("Created {}", stamp::full(created))));
            }
        }

        menu::sections([shown, held, disk, tab_rows, delete, about])
    }

    /// Drop an entry, file and all. Deleting the session on screen lands on
    /// the first remaining entry in the sidebar's displayed order.
    pub(crate) fn delete_entry(
        &mut self,
        entry: &Row,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let located = entry.located(self.workspace.read(cx));
        let landing_project = match (entry, located) {
            (
                Row::Entry {
                    showing: Showing::Session(_),
                    ..
                },
                Some((project, _)),
            ) if self.lit(cx).in_front(entry, self.workspace.read(cx)) => Some(project),
            _ => None,
        };
        let member = entry.member(self.workspace.read(cx), cx);
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
                    Showing::Session(_) | Showing::App(_) => {}
                },
                (Row::Group(Group::Space(id)), _) => workspace.delete_space_id(id, cx),
                (Row::Entry { .. }, None)
                | (Row::Group(Group::Project(_)) | Row::Archive(_) | Row::Heading(_), _) => {}
            }
        });
        if let Some(project) = landing_project
            && let Some(landing) = self
                .list(cx)
                .entries(project)
                .into_iter()
                .find(|row| !matches!(row, Row::Archive(_)))
        {
            self.open_row(&landing, window, cx);
            self.sidebar
                .update(cx, |sidebar, cx| sidebar.reveal(&landing, cx));
        }
        cx.notify();
    }

    /// Put the name field on an entry, in the place `host` names: the band's
    /// own field for the band's menu, the sidebar's for every other.
    ///
    /// A board is named by two things at once, so it opens its identity panel
    /// instead — which lives under the band, so the board is brought to the
    /// front first. One way to name a board, wherever you asked from.
    fn rename_entry(
        &mut self,
        entry: &Row,
        host: &Host,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Row::Entry {
            showing: Showing::Board(id),
            ..
        } = entry
        {
            if let Some((project, ix)) = entry.located(self.workspace.read(cx)) {
                self.open_board(project, ix, window, cx);
                self.open_info(id, window, cx);
            }
            return;
        }
        let Some(what) = Renaming::of(entry) else {
            return;
        };
        match host {
            Host::Header => self.start_rename(what, window, cx),
            Host::Sidebar | Host::Library | Host::Tab(_) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.start_rename(what, window, cx)),
        }
    }

    /// Put an entry away, or bring it back. Where the flag lives is each
    /// kind's own business — a board's file, an article's properties, a row in
    /// the store — and the sidebar asks for it the same way.
    pub(crate) fn archive_entry(
        &mut self,
        entry: &Row,
        archived: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read before the flag moves: once the entry is away it is no longer
        // what any pane is on.
        let landing = (archived && self.lit(cx).in_front(entry, self.workspace.read(cx)))
            .then(|| {
                entry
                    .located(self.workspace.read(cx))
                    .map(|(project, _)| project)
            })
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
                .filter_map(|member| Row::of_member(member, workspace, cx))
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
            let member = entry.member(self.workspace.read(cx), cx);
            self.put_away_arranged(member.as_ref(), window, cx);
            self.workspace.update(cx, |workspace, cx| {
                if let Some(project) = workspace.project_at(project) {
                    workspace.unpin_entry(project, showing.clone(), cx);
                }
                if let Some(member) = member {
                    workspace.drop_from_spaces(&member, cx);
                }
            });
        }
        let article = match (showing, entry.located(self.workspace.read(cx))) {
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
            Showing::App(_) => {}
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

    /// Put the pane on the first entry the project still lists. What the pane
    /// falls back to when the entry it was on is put away — [`Self::entries`]
    /// sorts the archived below the divider, so the first row is one still in
    /// hand or the divider itself.
    fn open_top_entry(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(top) = self
            .list(cx)
            .entries(project)
            .into_iter()
            .find(|row| row.showing().is_some())
        else {
            return;
        };
        self.open_row(&top, window, cx);
    }

    /// The field, in the row's place.
    pub(crate) fn name_field(&self, _: &mut Context<Self>) -> AnyElement {
        self.name_field.clone().into_any_element()
    }

    /// Put the band's field on `what`.
    pub(crate) fn start_rename(
        &mut self,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = what.label(self.workspace.read(cx), cx);
        self.name_field
            .update(cx, |field, cx| field.start(label, Case::Mixed, window, cx));
        // See [`Cydonia::open_info`] — the other way round.
        self.info = None;
        self.renaming = Some(what);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| what.commit(name, workspace, cx));
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
