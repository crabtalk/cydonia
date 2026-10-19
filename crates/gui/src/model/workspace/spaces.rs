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
use super::*;
use crate::model::spaces::{Shelf, Store as _};
use artifact::{
    kind::Kind,
    location::Location,
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
    /// The space the window is arranged by, if one is open.
    pub fn active_space(&self) -> Option<&Space> {
        self.spaces.get(self.space?)
    }

    pub fn active_space_mut(&mut self) -> Option<&mut Space> {
        self.spaces.get_mut(self.space?)
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

    /// Carry a space to another place in the list. `space` follows the one
    /// it points at rather than the index it sits on, the way `active` does
    /// for projects — see [`Workspace::move_project`].
    pub fn move_space(&mut self, id: &str, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.space_ix(id) else {
            return;
        };
        if from == to || to >= self.spaces.len() {
            return;
        }
        let space = self.spaces.remove(from);
        self.spaces.insert(to, space);
        self.space = self.space.map(|at| match at {
            at if at == from => to,
            at if from < to && (from..=to).contains(&at) => at - 1,
            at if to < from && (to..=from).contains(&at) => at + 1,
            at => at,
        });
        self.save();
        cx.notify();
    }

    /// Read every space back off disk. Called at launch, since nothing else
    /// writes them.
    pub fn reload_spaces(&mut self, cx: &mut Context<Self>) {
        let open = self.active_space().map(|space| space.id.clone());
        let order: Vec<String> = self.spaces.iter().map(|at| at.id.clone()).collect();
        self.spaces = Self::in_order(Shelf::all(), &order);
        self.space = open.and_then(|id| self.spaces.iter().position(|at| at.id == id));
        cx.notify();
    }

    /// Show a space. The projects its panes are in are opened if they are not
    /// already — an arrangement is a working context, and half of one is not
    /// what was asked for.
    pub fn open_space(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(space) = self.spaces.get(ix) else {
            return;
        };
        let wanted: Vec<PathBuf> = space
            .entries()
            .into_iter()
            .filter_map(|member| member.project()?.local().map(Path::to_path_buf))
            .filter(|path| !self.projects.iter().any(|open| open.path == *path))
            .filter(|path| path.is_dir())
            .collect();
        for path in wanted {
            self.open_project(path, cx);
        }
        self.space = Some(ix);
        self.bring_up_space(cx);
        self.save();
        cx.notify();
    }

    /// Leave the arrangement, which is what opening any single entry does.
    pub fn leave_space(&mut self) {
        self.space = None;
    }

    /// Put `arriving` beside `target`, making the space that holds them if
    /// there is not one yet.
    ///
    /// This is the whole of how a space is born: the first drag onto a pane
    /// mints one over the entry already there, and every drag after it lands
    /// in the same space.
    pub fn arrange(
        &mut self,
        target: &Member,
        arriving: &Member,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let at = match self.space {
            Some(at) if at < self.spaces.len() => at,
            // The first drag. The space starts as the pane that was there,
            // which is what the arrival is being put beside.
            _ => {
                let space = Shelf::create("", target.clone())?;
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
        if !space.insert(target, arriving, side) {
            return None;
        }
        Shelf::save(space);
        self.space = Some(at);
        self.save();
        cx.notify();
        Some(at)
    }

    /// Put `members` into the space with this id, as tabs of the pane its
    /// first entry is in, or into a new space over the first of them for
    /// `None`. Each leaves whatever other space it was in. Answers the
    /// space's index.
    pub fn gather(
        &mut self,
        space: Option<&str>,
        members: &[Member],
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let keep = match space {
            Some(id) => id.to_owned(),
            None => {
                let made = Shelf::create("", members.first()?.clone())?;
                let id = made.id.clone();
                self.spaces.insert(0, made);
                self.space = self.space.map(|open| open + 1);
                id
            }
        };
        for member in members {
            // By id, because an eviction can take a space with it.
            self.evict_from_spaces(member, &keep, cx);
            let at = self.space_ix(&keep)?;
            let space = &mut self.spaces[at];
            let target = space.entries().into_iter().next()?;
            if target != *member && space.stack(&target, member) {
                Shelf::save(space);
            }
        }
        self.save();
        cx.notify();
        self.space_ix(&keep)
    }

    /// Move a tab to where `to` sits in the strip of the pane holding both.
    pub fn reorder_tab(&mut self, moving: &Member, to: &Member, cx: &mut Context<Self>) {
        self.edit_space(cx, |space| space.reorder(moving, to));
    }

    /// Put an arriving entry into the pane holding `target`, as a tab — a
    /// drop on a pane's bar rather than on its edge. With no space open, the
    /// space starts as `target`'s pane, as in [`Self::arrange`].
    pub fn stack_pane(&mut self, target: &Member, arriving: &Member, cx: &mut Context<Self>) {
        if target == arriving {
            return;
        }
        let at = match self.space {
            Some(at) if at < self.spaces.len() => at,
            _ => {
                let Some(space) = Shelf::create("", target.clone()) else {
                    return;
                };
                self.spaces.insert(0, space);
                0
            }
        };
        // An entry is in one space at a time, the same rule [`Self::arrange`]
        // follows for the same reason. By id, because an eviction can take a
        // space with it.
        let keep = self.spaces[at].id.clone();
        self.evict_from_spaces(target, &keep, cx);
        self.evict_from_spaces(arriving, &keep, cx);
        self.space = self.spaces.iter().position(|space| space.id == keep);
        self.save();
        self.edit_space(cx, |space| space.stack(target, arriving));
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
            if self.spaces[ix].entries().len() <= 2 {
                self.delete_space(ix, cx);
                continue;
            }
            // A space that says it holds the member but will not give it up
            // would spin this loop, so the refusal ends it.
            if !self.spaces[ix].remove(member) {
                return;
            }
            Shelf::save(&mut self.spaces[ix]);
            cx.notify();
        }
    }

    /// The entries the pane holding this one draws, in strip order. One entry
    /// for a pane holding one thing, and none at all where no space is open.
    pub fn stack_of(&self, entry: &Member) -> Vec<Member> {
        self.active_space()
            .map(|space| space.stack_of(entry))
            .unwrap_or_default()
    }

    /// The tab the pane holding this entry shows — see [`Space::front_of`].
    pub fn front_of(&self, entry: &Member) -> Option<Member> {
        self.active_space()?.front_of(entry)
    }

    /// Bring a tab to the front of its pane in the open space.
    pub fn set_front(&mut self, tab: &Member, cx: &mut Context<Self>) {
        self.edit_space(cx, |space| space.set_front(tab));
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

    /// Close one pane, or one of its tabs.
    ///
    /// Closing down to one entry closes the space: an arrangement of one thing
    /// is not an arrangement, and leaving the file behind would put a row in
    /// the sidebar for something the window is no longer doing. Entries, not
    /// panes: a pane of two tabs beside a pane of one is still an arrangement
    /// once the lone pane goes, and stays a space holding both tabs.
    ///
    /// The entry that would have been left alone is what the window is put on,
    /// so the entry you were keeping stays in front — and is returned, so the
    /// caller can put the single pane on it. Nothing while the arrangement
    /// survives, which leaves the panes to say what they show.
    pub fn close_pane(
        &mut self,
        entry: &Member,
        cx: &mut Context<Self>,
    ) -> Option<(Member, Showing)> {
        let space = self.active_space()?;
        // Exactly one entry goes, whether it was a pane's last or one tab of
        // several, so what is left is what the space held less that one.
        if space.entries().len() > 2 {
            self.edit_space(cx, |space| space.remove(entry));
            return None;
        }
        let survivor = space
            .entries()
            .into_iter()
            .find(|member| member != entry)
            .and_then(|member| Some((member.clone(), self.showing_of(&member, cx)?)));

        if let Some(at) = self.space {
            self.delete_space(at, cx);
        }
        let (member, (project, showing)) = survivor?;
        self.select_showing(project, showing.clone(), cx);
        Some((member, showing))
    }

    /// Take an entry out of whatever space holds it, open or not.
    ///
    /// [`Self::close_pane`] is the gesture's version of this and works on the
    /// space in front; this one is for an entry that is going away from the
    /// list altogether, which can be holding a pane in an arrangement nobody
    /// is looking at. Taking the last pane out takes the space with it, the
    /// same rule and for the same reason.
    pub fn drop_from_spaces(&mut self, member: &Member, cx: &mut Context<Self>) {
        let Some(ix) = self.space_holding(member) else {
            return;
        };
        let Some(space) = self.spaces.get_mut(ix) else {
            return;
        };
        // Entries, not panes: what is left can be one pane holding tabs, which
        // is still an arrangement.
        if space.entries().len() <= 2 {
            self.delete_space(ix, cx);
            return;
        }
        if space.remove(member) {
            Shelf::save(space);
            cx.notify();
        }
    }

    /// Take every archived entry out of the space holding it, by the rule
    /// [`Self::drop_from_spaces`] follows: a space left with one pane goes.
    pub(super) fn drop_archived_from_spaces(&mut self, cx: &mut Context<Self>) {
        let archived: Vec<Member> = self
            .spaces
            .iter()
            .flat_map(Space::entries)
            .filter(|member| self.archived(member, cx))
            .collect();
        for member in archived {
            self.drop_from_spaces(&member, cx);
        }
    }

    /// Whether the entry a member names is archived. One no open project lists
    /// is not.
    fn archived(&self, member: &Member, cx: &App) -> bool {
        let Some(open) = self
            .projects
            .iter()
            .find(|open| member.project().and_then(Location::local) == Some(open.path.as_path()))
        else {
            return false;
        };
        let id = member.id();
        match member.kind() {
            Kind::Session => open
                .sessions
                .iter()
                .map(|held| held.chat.read(cx))
                .any(|chat| chat.closed && chat.record.as_deref() == Some(id)),
            Kind::Board => open
                .boards
                .iter()
                .any(|held| held.id == id && held.board.read(cx).archived),
            Kind::Article => open
                .articles
                .iter()
                .any(|article| article.archived && article.id == id),
            Kind::Table => open
                .tables
                .iter()
                .any(|table| table.archived && table.key == id),
            Kind::App => false,
        }
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
        Shelf::remove(&self.spaces.remove(ix).id);
        self.space = self
            .space
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
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

    /// The space a given entry is a member of, if any — what the sidebar
    /// lists it under. One put away holds nothing: its members are listed
    /// where they would be without it.
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
        let mut live: Vec<Member> = Vec::new();
        for open in &self.projects {
            let location = open.location();
            for chat in open.chats(cx) {
                if let Some(record) = chat.record.clone() {
                    live.push(Member::new(location.clone(), Kind::Session, record));
                }
            }
            for board in &open.boards {
                live.push(Member::new(location.clone(), Kind::Board, board.id.clone()));
            }
            for article in &open.articles {
                live.push(Member::new(
                    location.clone(),
                    Kind::Article,
                    article.id.clone(),
                ));
            }
            for table in &open.tables {
                live.push(Member::new(
                    location.clone(),
                    Kind::Table,
                    table.key.clone(),
                ));
            }
        }
        live.extend(self.apps.iter().map(|held| Member::app(held.id.clone())));
        let open: Vec<Location> = self.projects.iter().map(Project::location).collect();
        // A table is not missing from a project whose tables have not landed.
        let unlisted: Vec<Location> = self
            .projects
            .iter()
            .filter(|open| !open.tables_listed())
            .map(Project::location)
            .collect();
        for space in &mut self.spaces {
            let gone: Vec<Member> = space
                .entries()
                .into_iter()
                .filter(|member| {
                    member
                        .project()
                        .is_none_or(|project| open.iter().any(|at| at == project))
                        && !live.contains(member)
                        && !(member.kind() == Kind::Table
                            && member
                                .project()
                                .is_some_and(|project| unlisted.contains(project)))
                })
                .collect();
            if !gone.is_empty() && space.prune(&gone) {
                Shelf::save(space);
            }
        }
        cx.notify();
    }

    /// What the member names, and which open project it is in. Nothing for one
    /// whose project is shut, or which has gone since the space named it —
    /// that draws as an empty pane rather than a panic.
    pub fn showing_of(&self, member: &Member, cx: &App) -> Option<(usize, Showing)> {
        let at = self.projects.iter().position(|open| {
            member.project().and_then(Location::local) == Some(open.path.as_path())
        })?;
        let open = self.projects.get(at)?;
        let showing = open.stored(member.kind(), member.id(), cx)?;
        Some((at, showing))
    }

    /// The member that names what a pane is on, so an entry opened from the
    /// sidebar can be put into a space.
    ///
    /// Nothing for a session with no file yet: a space names entries on disk,
    /// and one that has had no turn is not there to be named.
    pub fn member_of(&self, project: usize, showing: Showing, cx: &App) -> Option<Member> {
        let Showing::Session(id) = showing else {
            return self.filed_member(project, showing).map(Member::from);
        };
        let open = self.projects.get(project)?;
        let record = open.session(id, cx)?.filed()?.to_owned();
        Some(Member::new(open.location(), Kind::Session, record))
    }

    /// [`Self::member_of`] for a board, an article or a table, whose id is
    /// its file. Nothing for a session.
    fn filed_member(&self, project: usize, showing: Showing) -> Option<Entry> {
        let open = self.projects.get(project)?;
        let project = open.location();
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
        let id = self.projects.get(project)?.boards.get(ix)?.id.clone();
        self.filed_member(project, Showing::Board(id))
            .map(Member::from)
    }

    /// The member naming the article listed at `ix` in `project`.
    pub fn article_member(&self, project: usize, ix: usize) -> Option<Member> {
        let id = self.projects.get(project)?.articles.get(ix)?.id.clone();
        self.filed_member(project, Showing::Article(id))
            .map(Member::from)
    }

    /// The member naming the table listed at `ix` in `project`.
    pub fn table_member(&self, project: usize, ix: usize) -> Option<Member> {
        let key = self.projects.get(project)?.tables.get(ix)?.key.clone();
        self.filed_member(project, Showing::Table(key))
            .map(Member::from)
    }

    /// The program of the app a pane's member names, where it is listed.
    pub fn program_of(
        &self,
        member: &Member,
        cx: &App,
    ) -> Option<&bezel::gpui::Entity<crate::model::apps::Program>> {
        match member {
            Member::App { app } => self.program(app, cx),
            Member::Entry { .. } => None,
        }
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
        match self.showing_of(member, cx)? {
            (project, Showing::Board(id)) => {
                self.board_in(project, self.projects.get(project)?.board_ix(&id)?, cx)
            }
            _ => None,
        }
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
        match self.showing_of(member, cx)? {
            (project, Showing::Article(id)) => {
                self.article_in(project, self.projects.get(project)?.article_ix(&id)?)
            }
            _ => None,
        }
    }

    /// The table one pane is showing, the way [`Self::board_of`] answers for a
    /// board.
    pub fn table_of(&self, member: Option<&Member>, cx: &App) -> Option<&Table> {
        let Some(member) = member else {
            return self.active_table();
        };
        match self.showing_of(member, cx)? {
            (project, Showing::Table(key)) => {
                self.table_in(project, self.projects.get(project)?.table_ix(&key)?)
            }
            _ => None,
        }
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

    pub fn session_in<'a>(&self, project: usize, id: u64, cx: &'a App) -> Option<&'a ChatSession> {
        self.projects.get(project)?.session(id, cx)
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
                    open.reload_page(cx);
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
        let Some(space) = self.space.and_then(|ix| self.spaces.get_mut(ix)) else {
            return;
        };
        if edit(space) {
            Shelf::save(space);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/spaces.rs"]
mod space_tests;
