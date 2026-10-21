//! The spaces this machine holds: which entries are on screen together, and
//! how they are arranged.
//!
//! A continuation of [`Workspace`]'s one `impl`, the way [`super::boards`] is.
//!
//! A space is not a project's. It names entries by the project they are in
//! and the id they have there, so one arrangement can hold a session from one
//! repository beside a board from another — which is why the window owns them
//! and why they are kept beside the config rather than in any `.cydonia/`. See
//! [`crate::model::spaces`] for where they are written.
//!
//! There is no `new_space` a menu calls: a space is made by dragging a
//! second entry onto the one already open — see [`Workspace::arrange`].
use super::tabs::Slot;
use super::*;
use crate::model::spaces::{Shelf, Store as _};
use artifact::{
    kind::Kind,
    space::{Entry, Member, Side, Space},
};

/// What a pane shows, resolved from the member a space names: an entry of
/// one project, by the id it has there.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Showing {
    Session(u64),
    /// A board's id.
    Board(String),
    /// An article's id.
    Article(String),
    /// A table's key.
    Table(String),
}

impl Showing {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Session(_) => Kind::Session,
            Self::Board(_) => Kind::Board,
            Self::Article(_) => Kind::Article,
            Self::Table(_) => Kind::Table,
        }
    }
}

impl Workspace {
    /// Where the space whose tab is in front of the strip is listed.
    pub fn space(&self) -> Option<usize> {
        self.space_ix(self.front_slot()?.space()?)
    }

    /// The space whose tab is in front of the strip.
    pub fn active_space(&self) -> Option<&Space> {
        self.spaces.get(self.space()?)
    }

    pub fn active_space_mut(&mut self) -> Option<&mut Space> {
        let ix = self.space()?;
        self.spaces.get_mut(ix)
    }

    /// Put `spaces` in the order `ids` names them, with anything that list
    /// does not name in front, newest first.
    ///
    /// A space's file carries no place in the list: [`Shelf::all`] reads them
    /// back by the time they were written, and the hand order is kept beside
    /// the rest of the window's bookkeeping — see
    /// [`crate::model::state::State::spaces`].
    pub(super) fn in_order(spaces: Vec<Space>, ids: &[String]) -> Vec<Space> {
        let mut spaces = spaces;
        spaces.sort_by_key(|space| {
            ids.iter()
                .position(|id| *id == space.id)
                .map_or(0, |at| at + 1)
        });
        spaces
    }

    /// Whether a section of the sidebar has its rows hidden, by name.
    pub fn section_folded(&self, name: &str) -> bool {
        self.folded_sections.contains(name)
    }

    /// Hide a section's rows in the sidebar, or show them again.
    pub fn toggle_section(&mut self, name: &str, cx: &mut Context<Self>) {
        if !self.folded_sections.remove(name) {
            self.folded_sections.insert(name.to_owned());
        }
        self.save();
        cx.notify();
    }

    /// Whether a space's members are hidden in the sidebar.
    pub fn space_folded(&self, id: &str) -> bool {
        self.folded_spaces.contains(id)
    }

    /// The space's members in the order the sidebar lists them.
    pub fn listed_members(&self, space: &artifact::space::Space) -> Vec<Member> {
        let mut members = space.entries();
        if let Some(order) = self.space_order.get(&space.id) {
            members.sort_by_key(|member| order.iter().position(|at| at == member).map(|at| at + 1));
        }
        members
    }

    /// Write down the order the sidebar lists a space's members in.
    pub fn set_space_order(&mut self, id: &str, order: Vec<Member>, cx: &mut Context<Self>) {
        self.space_order.insert(id.to_owned(), order);
        self.save();
        cx.notify();
    }

    /// Hide a space's members in the sidebar, or show them again.
    pub fn toggle_space(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.folded_spaces.remove(id) {
            self.folded_spaces.insert(id.to_owned());
        }
        self.save();
        cx.notify();
    }

    /// Where the space with `id` is listed.
    pub fn space_ix(&self, id: &str) -> Option<usize> {
        self.spaces.iter().position(|space| space.id == id)
    }

    /// The space with `id`.
    pub fn space_by_id(&self, id: &str) -> Option<&Space> {
        self.spaces.iter().find(|space| space.id == id)
    }

    /// Carry a space to another place in the list.
    pub fn move_space(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.space_ix(id) else {
            return;
        };
        if from == to || to >= self.spaces.len() {
            return;
        }
        let space = self.spaces.remove(from);
        self.spaces.insert(to, space);
        self.save();
        cx.notify();
    }

    /// Show a space: its tab in front of the strip. The projects its panes are
    /// in are opened if they are not already — an arrangement is a working
    /// context, and half of one is not what was asked for.
    pub fn open_space(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(space) = self.spaces.get(ix) else {
            return;
        };
        let wanted: Vec<PathBuf> = space
            .entries()
            .into_iter()
            .filter_map(|member| member.project().map(Path::to_path_buf))
            .filter(|path| !self.projects.iter().any(|open| open.path == *path))
            .filter(|path| path.is_dir())
            .collect();
        let id = space.id.clone();
        for path in wanted {
            self.open_project(path, cx);
        }
        self.open_space_tab(&id);
        self.bring_up_space(cx);
        self.save();
        cx.notify();
    }

    /// Put `arriving` beside `target`, making the space that holds them if
    /// there is not one yet.
    ///
    /// This is the whole of how a space is born: the first drag onto the edge
    /// of an entry in front mints one over it, in its tab's place, and every
    /// drag after it lands in the same space.
    pub fn arrange(
        &mut self,
        target: &Member,
        arriving: &Member,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.gather(target, arriving, cx, |space| {
            space.insert(target, arriving, side)
        })
    }

    /// Put `arriving` in front of the stack of the pane holding `target`,
    /// making the space that holds them if there is not one yet, as
    /// [`Self::arrange`] does.
    pub fn stack_pane(
        &mut self,
        target: &Member,
        arriving: &Member,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.gather(target, arriving, cx, |space| {
            space.stack(target, arriving);
            space.set_front(arriving);
            space.stack_of(target).contains(arriving)
        })
    }

    /// Bring `target` and `arriving` into the space in front, or one minted
    /// over `target`, and `place` the arrival there. The space's tab takes
    /// `target`'s place in the strip, and both entries' own tabs go.
    fn gather(
        &mut self,
        target: &Member,
        arriving: &Member,
        cx: &mut Context<Self>,
        place: impl FnOnce(&mut Space) -> bool,
    ) -> Option<usize> {
        let at = match self.space() {
            Some(at) => at,
            // The first drag. The space starts as the pane that was there,
            // which is what the arrival is being put beside.
            _ => {
                let space = Shelf::create(target.clone())?;
                self.spaces.insert(0, space);
                0
            }
        };
        // Both of them, not only the arrival: a space minted over a target
        // another one already holds would put that entry in two at once. By id,
        // because an eviction can take a space with it.
        let keep = self.spaces.get(at)?.id.clone();
        self.evict_from_spaces(target, &keep, cx);
        self.evict_from_spaces(arriving, &keep, cx);
        let at = self.spaces.iter().position(|space| space.id == keep)?;
        let space = self.spaces.get_mut(at)?;
        if !place(space) {
            return None;
        }
        Shelf::save(space);
        let id = space.id.clone();
        self.replace_slot(
            &Slot::Entry(target.clone()),
            Slot::Space { space: id.clone() },
        );
        self.open_space_tab(&id);
        self.drop_tabs(&[target.clone(), arriving.clone()], cx);
        self.save();
        cx.notify();
        Some(at)
    }

    /// Open an arriving entry as a tab of the strip — a drop on a pane but not
    /// on its edge. It leaves any space it was in.
    pub fn join_strip(&mut self, arriving: &Member, cx: &mut Context<Self>) {
        self.evict_from_spaces(arriving, "", cx);
        self.open_tab(arriving.clone(), super::Opening::Kept, cx);
    }

    /// Take an entry out of every space but the one named.
    ///
    /// An entry is in one space at a time, the way a pane is in one tmux
    /// window: dragging it into another moves it. What is left with one entry
    /// is deleted rather than kept — an arrangement of one is not an arrangement,
    /// which is the rule [`Self::close_pane`] and [`Self::drop_from_spaces`]
    /// already follow.
    ///
    /// By id and not by index, because a space dropped in here shifts every
    /// index past it.
    fn evict_from_spaces(&mut self, member: &Member, keep: &str, cx: &mut Context<Self>) {
        while let Some(ix) = self
            .spaces
            .iter()
            .position(|space| space.id != keep && space.contains(member))
        {
            // A space that says it holds the member but will not give it up
            // would spin this loop, so the refusal ends it.
            if !self.leave_space(ix, member, cx) {
                return;
            }
        }
    }

    /// Take `member` out of the space at `ix` and write it back, or delete the
    /// space where it holds two entries or fewer. Answers `false` where the
    /// space does not give the member up.
    fn leave_space(&mut self, ix: usize, member: &Member, cx: &mut Context<Self>) -> bool {
        let Some(space) = self.spaces.get_mut(ix) else {
            return false;
        };
        if space.entries().len() <= 2 {
            self.delete_space(ix, cx);
            return true;
        }
        if !space.remove(member) {
            return false;
        }
        Shelf::save(space);
        cx.notify();
        true
    }

    /// Bring an entry to the front: of its pane's stack where a space holds
    /// it, of the strip otherwise.
    pub fn set_front(&mut self, member: &Member, cx: &mut Context<Self>) {
        let Some(ix) = self.space_holding(member) else {
            self.front_tab(&Slot::Entry(member.clone()), cx);
            return;
        };
        let space = &mut self.spaces[ix];
        if space.set_front(member) {
            Shelf::save(space);
            cx.notify();
        }
    }

    /// Record the pane on `entry` as the open space's focused one. Writes the
    /// space back and notifies nobody.
    pub fn set_focused(&mut self, entry: &Member) {
        let Some(space) = self.active_space_mut() else {
            return;
        };
        if space.focus(entry) {
            Shelf::save(space);
        }
    }

    /// The entries of the open space's pane holding `member`, in stack order.
    /// Empty where the open space does not hold it.
    pub fn stack_of(&self, member: &Member) -> Vec<Member> {
        self.active_space()
            .map(|space| space.stack_of(member))
            .unwrap_or_default()
    }

    /// The pane across the seam on this side of the one given, if there is one
    /// — what says whether a move that way is on offer at all.
    pub fn neighbour_pane(&self, entry: &Member, side: Side) -> Option<Member> {
        self.active_space()?.tree.neighbour(entry, side)
    }

    /// Exchange a pane with the one across the seam on that side. The
    /// arrangement keeps its shape and its sizes.
    pub fn move_pane(&mut self, entry: &Member, side: Side, cx: &mut Context<Self>) -> bool {
        let Some(across) = self.neighbour_pane(entry, side) else {
            return false;
        };
        let mut moved = false;
        self.edit_space(cx, |space| {
            moved = space.tree.swap(entry, &across);
            moved
        });
        moved
    }

    /// Move a pane already on screen to another pane's edge.
    pub fn relocate_pane(
        &mut self,
        entry: &Member,
        target: &Member,
        side: Side,
        cx: &mut Context<Self>,
    ) {
        self.edit_space(cx, |space| space.relocate(entry, target, side));
    }

    /// Close one pane of the space in front, or with an entry in front, that
    /// entry's tab.
    ///
    /// Closing down to one entry closes the space: an arrangement of one thing
    /// is not an arrangement, and leaving the file behind would put a tab on
    /// the strip for something the window is no longer doing. Entries, not
    /// panes: a leaf stacking two entries beside a pane of one is still an
    /// arrangement once that pane goes.
    ///
    /// The entry that would have been left alone takes the space's tab, and is
    /// returned. Nothing while the arrangement survives, which leaves the
    /// panes to say what they show.
    pub fn close_pane(
        &mut self,
        entry: &Member,
        cx: &mut Context<Self>,
    ) -> Option<(Member, Showing)> {
        let Some(space) = self.active_space().filter(|space| space.contains(entry)) else {
            self.close_tab(entry, cx);
            return None;
        };
        // Exactly one entry goes, whether it was a pane's last or one tab of
        // several, so what is left is what the space held less that one.
        if space.entries().len() > 2 {
            let ix = self.space()?;
            self.leave_space(ix, entry, cx);
            return None;
        }
        let id = space.id.clone();
        let survivor = space
            .entries()
            .into_iter()
            .find(|member| member != entry)
            .and_then(|member| Some((member.clone(), self.showing_of(&member, cx)?)));
        if let Some((member, _)) = &survivor {
            self.replace_slot(
                &Slot::Space { space: id.clone() },
                Slot::Entry(member.clone()),
            );
        }
        self.delete_space_id(&id, cx);
        let (member, (project, showing)) = survivor?;
        self.select_showing(project, showing.clone(), cx);
        Some((member, showing))
    }

    /// Take an entry out of whatever space holds it, open or not.
    ///
    /// [`Self::close_pane`] is the gesture's version of this and works on the
    /// space in front; this one is for an entry that is going away from the
    /// list altogether, which can be holding a pane in an arrangement nobody
    /// is looking at. A space left with one entry is deleted, as there.
    pub fn drop_from_spaces(&mut self, member: &Member, cx: &mut Context<Self>) {
        self.drop_tabs(std::slice::from_ref(member), cx);
        if let Some(ix) = self.space_holding(member) {
            self.leave_space(ix, member, cx);
        }
    }

    /// Take every archived entry out of the space holding it, by the rule
    /// [`Self::drop_from_spaces`] follows: a space left with one entry goes.
    pub(super) fn drop_archived_from_spaces(&mut self, cx: &mut Context<Self>) {
        let archived: Vec<Member> = self
            .spaces
            .iter()
            .flat_map(Space::entries)
            .chain(self.tab_strip())
            .filter(|member| self.archived(member, cx))
            .collect();
        for member in archived {
            self.drop_from_spaces(&member, cx);
        }
    }

    /// Whether the entry a member names is archived. One no open project lists
    /// is not.
    fn archived(&self, member: &Member, cx: &App) -> bool {
        self.showing_of(member, cx)
            .and_then(|(at, showing)| self.projects.get(at)?.facts(&showing, cx))
            .is_some_and(|facts| facts.archived)
    }

    /// Stand one pane over the others, or put it back.
    pub fn zoom_pane(&mut self, entry: &Member, cx: &mut Context<Self>) {
        self.edit_space(cx, |space| {
            space.zoom(entry);
            true
        });
    }

    /// Drop the space: the file goes with it. The entries it arranged are
    /// left alone — a space holds none of them.
    pub fn delete_space(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.spaces.len() {
            return;
        }
        let id = self.spaces.remove(ix).id;
        Shelf::remove(&id);
        self.take_slot(&Slot::Space { space: id }, cx);
        self.save();
        cx.notify();
    }

    /// Drop the space named, wherever it has got to in the list.
    ///
    /// By id and not by place: archiving what a space arranges takes each
    /// member out of it as it goes — see [`Self::drop_from_spaces`] — so an
    /// index read before that walk names some other space by the end of it,
    /// or nothing.
    pub fn delete_space_id(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.space_ix(id) {
            self.delete_space(ix, cx);
        }
    }

    pub fn rename_space(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        if let Some(space) = self.spaces.iter_mut().find(|space| space.id == id) {
            space.name = name.trim().to_owned();
            Shelf::save(space);
            cx.notify();
        }
    }

    /// The space a given entry is a member of, if any.
    pub fn space_holding(&self, member: &Member) -> Option<usize> {
        self.spaces.iter().position(|space| space.contains(member))
    }

    /// Drop members whose entries have gone. A space names entries and holds
    /// none of them, so one deleted from the sidebar leaves a pane pointing at
    /// nothing until this runs.
    ///
    /// Only for projects that are open: a member in a project this window has
    /// never opened is not missing, only out of reach.
    pub fn prune_spaces(&mut self, cx: &mut Context<Self>) {
        let open: Vec<PathBuf> = self.projects.iter().map(|open| open.path.clone()).collect();
        let gone: Vec<Vec<Member>> = self
            .spaces
            .iter()
            .map(|space| {
                space
                    .entries()
                    .into_iter()
                    .filter(|member| {
                        member
                            .project()
                            .is_none_or(|project| open.iter().any(|at| at == project))
                            && !self.lists(member, cx)
                    })
                    .collect()
            })
            .collect();
        for (space, gone) in self.spaces.iter_mut().zip(gone) {
            if !gone.is_empty() && space.prune(&gone) {
                Shelf::save(space);
            }
        }
        // A tab is on an entry of an open project, or it goes.
        let gone: Vec<Member> = self
            .tab_strip()
            .into_iter()
            .filter(|member| !self.lists(member, cx))
            .collect();
        self.drop_tabs(&gone, cx);
        cx.notify();
    }

    /// Where the open project `member` is in is listed, found by the path the
    /// member names exactly. Nothing for an app.
    pub fn member_project(&self, member: &Member) -> Option<usize> {
        self.projects
            .iter()
            .position(|open| Some(open.path.as_path()) == member.project())
    }

    /// Whether what `member` names is here to show: an app that is installed,
    /// or an entry an open project lists.
    pub(super) fn lists(&self, member: &Member, cx: &App) -> bool {
        match member {
            Member::App { app } => self.apps.iter().any(|held| held.id == *app),
            Member::Entry(_) => self.showing_of(member, cx).is_some(),
        }
    }

    /// What the member names, and which open project it is in. Nothing for one
    /// whose project is shut, or which has gone since the space named it —
    /// that draws as an empty pane rather than a panic.
    pub fn showing_of(&self, member: &Member, cx: &App) -> Option<(usize, Showing)> {
        let at = self.member_project(member)?;
        let open = self.projects.get(at)?;
        let showing = open.stored(member.kind(), member.id(), cx)?;
        Some((at, showing))
    }

    /// The member that names what a pane is on, so an entry opened from the
    /// sidebar can be put into a space or a tab.
    ///
    /// Nothing for a session with no record id yet — see
    /// [`Self::mint_record`].
    pub fn member_of(&self, project: usize, showing: Showing, cx: &App) -> Option<Member> {
        let Showing::Session(id) = showing else {
            return self.filed_member(project, showing).map(Member::from);
        };
        let open = self.projects.get(project)?;
        let record = open.session(id, cx)?.record.clone()?;
        Some(Member::new(open.path.clone(), Kind::Session, record))
    }

    /// [`Self::member_of`] for a board, an article or a table, whose id is
    /// its file. Nothing for a session.
    fn filed_member(&self, project: usize, showing: Showing) -> Option<Entry> {
        let open = self.projects.get(project)?;
        let project = open.path.clone();
        let (kind, id) = match showing {
            Showing::Session(_) => return None,
            Showing::Board(id) => {
                open.board_ix(&id)?;
                (Kind::Board, id)
            }
            Showing::Article(id) => (
                Kind::Article,
                open.articles[open.article_ix(&id)?].id.clone(),
            ),
            Showing::Table(key) => {
                open.table_ix(&key)?;
                (Kind::Table, key)
            }
        };
        Some(Entry { project, kind, id })
    }

    /// The member naming the board listed at `ix` in `project`.
    pub fn board_member(&self, project: usize, ix: usize) -> Option<Member> {
        let open = self.projects.get(project)?;
        let id = open.boards.get(ix)?.id.clone();
        Some(Member::new(open.path.clone(), Kind::Board, id))
    }

    /// The member naming the article listed at `ix` in `project`.
    pub fn article_member(&self, project: usize, ix: usize) -> Option<Member> {
        let open = self.projects.get(project)?;
        let id = open.articles.get(ix)?.id.clone();
        Some(Member::new(open.path.clone(), Kind::Article, id))
    }

    /// The member naming the table listed at `ix` in `project`.
    pub fn table_member(&self, project: usize, ix: usize) -> Option<Member> {
        let open = self.projects.get(project)?;
        let key = open.tables.get(ix)?.key.clone();
        Some(Member::new(open.path.clone(), Kind::Table, key))
    }

    /// Which open project `member` is in, and where it is listed among its
    /// kind there.
    fn located_member(&self, member: &Member, cx: &App) -> Option<(usize, usize)> {
        let at = self.member_project(member)?;
        let ix = self.projects[at].stored_ix(member.kind(), member.id(), cx)?;
        Some((at, ix))
    }

    /// The board a pane is on, by its place in its project. Named apart from
    /// [`Self::board_at`], which finds one by file wherever it is open.
    pub fn board_in<'a>(&self, project: usize, ix: usize, cx: &'a App) -> Option<&'a Board> {
        if !self.settings.features.boards {
            return None;
        }
        self.projects.get(project)?.board(ix, cx)
    }

    /// The board one pane is showing.
    ///
    /// The pane's own member rather than the project's selection, which
    /// answers for at most one of the boards a space can have on screen. No
    /// member is a window showing one entry, where the selection is the whole
    /// truth.
    pub fn board_of<'a>(&self, member: Option<&Member>, cx: &'a App) -> Option<&'a Board> {
        let Some(member) = member else {
            return self.active_board(cx);
        };
        if member.kind() != Kind::Board {
            return None;
        }
        let (project, ix) = self.located_member(member, cx)?;
        self.board_in(project, ix, cx)
    }

    /// The session one pane is showing, the way [`Self::board_of`] answers
    /// for a board.
    pub fn session_id_of(&self, member: Option<&Member>, cx: &App) -> Option<u64> {
        let Some(member) = member else {
            return self.active_id();
        };
        match self.showing_of(member, cx)? {
            (_, Showing::Session(id)) => Some(id),
            _ => None,
        }
    }

    /// The article one pane is showing, the way [`Self::board_of`] answers for
    /// a board.
    pub fn article_of(&self, member: Option<&Member>, cx: &App) -> Option<&Article> {
        let Some(member) = member else {
            return self.active_article();
        };
        if member.kind() != Kind::Article {
            return None;
        }
        let (project, ix) = self.located_member(member, cx)?;
        self.article_in(project, ix)
    }

    /// The table one pane is showing, the way [`Self::board_of`] answers for a
    /// board.
    pub fn table_of(&self, member: Option<&Member>, cx: &App) -> Option<&Table> {
        let Some(member) = member else {
            return self.active_table();
        };
        if member.kind() != Kind::Table {
            return None;
        }
        let (project, ix) = self.located_member(member, cx)?;
        self.table_in(project, ix)
    }

    pub fn article_in(&self, project: usize, ix: usize) -> Option<&Article> {
        self.projects.get(project)?.articles.get(ix)
    }

    pub fn table_in(&self, project: usize, ix: usize) -> Option<&Table> {
        if !self.settings.features.tables {
            return None;
        }
        self.projects.get(project)?.tables.get(ix)
    }

    /// The rows a table pane is on.
    pub fn page_in(&self, project: usize, ix: usize) -> Option<&Page> {
        if !self.settings.features.tables {
            return None;
        }
        let open = self.projects.get(project)?;
        open.pages.get(&open.tables.get(ix)?.key)
    }

    /// Put the project's selection on what a pane is showing.
    ///
    /// The four slots are what every command without a pane of its own reads,
    /// so this is what makes the focused pane the one they act on.
    ///
    /// The entry is brought up to what drawing it needs first — see
    /// [`Self::bring_up`].
    pub fn select_showing(&mut self, project: usize, showing: Showing, cx: &mut Context<Self>) {
        let drawable = self.bring_up(project, &showing, cx);
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        match &showing {
            Showing::Session(id) => open.active = Some(*id),
            Showing::Board(id) => {
                if drawable {
                    open.board = open.board_ix(id);
                }
            }
            Showing::Article(id) => open.article = open.article_ix(id),
            Showing::Table(key) => {
                if let Some(ix) = open.table_ix(key) {
                    open.table = Some(ix);
                    open.reload_page();
                }
            }
        }
        self.active = Some(project);
        if let Showing::Session(id) = showing {
            self.wake_session(id, cx);
        }
        cx.notify();
    }

    /// Bring an entry up to what drawing it in a pane needs, without selecting
    /// it. An article with no editor draws as the front door. `false` where
    /// the entry is not there to draw: no such project, or no such board.
    fn bring_up(&mut self, project: usize, showing: &Showing, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.projects.get_mut(project) else {
            return false;
        };
        match showing {
            Showing::Board(id) => open.board_ix(id).is_some(),
            Showing::Article(id) => {
                if let Some(ix) = open.article_ix(id) {
                    open.articles[ix].open(cx);
                }
                true
            }
            Showing::Session(_) | Showing::Table(_) => true,
        }
    }

    /// Bring every pane of the open space up to what drawing it needs — each
    /// pane's front entry, focused or not. For entering a space, which puts
    /// all of them on screen at once.
    pub(crate) fn bring_up_space(&mut self, cx: &mut Context<Self>) {
        let Some(space) = self.active_space() else {
            return;
        };
        let showing: Vec<_> = space
            .tree
            .fronts()
            .iter()
            .filter_map(|member| self.showing_of(member, cx))
            .collect();
        for (project, showing) in showing {
            self.bring_up(project, &showing, cx);
        }
    }

    /// Change the open space and write it back, if the change took.
    fn edit_space(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Space) -> bool) {
        if let Some(space) = self.active_space_mut()
            && edit(space)
        {
            Shelf::save(space);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/spaces.rs"]
mod space_tests;
