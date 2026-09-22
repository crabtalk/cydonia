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
use crate::model::spaces as store;
use artifact::space::{Kind as MemberKind, Member, Side, Space};

/// What a pane shows, resolved from the member a space names.
///
/// An index into the project's own list, not a borrow: the caller needs the
/// workspace back to draw with. Resolved fresh for each frame rather than kept
/// on the pane — an entry made or dropped beside the open one shifts every
/// index past it, and a pane holding a stale one would draw whatever slid into
/// its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Showing {
    /// A session, by the id it was minted with — stable for the life of the
    /// process, so this one is not an index.
    Session(u64),
    Board(usize),
    Article(usize),
    Table(usize),
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
    /// A space's file carries no place in the list: [`store::all`] reads them
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

    /// Carry a space to another place in the list. `space` follows the one
    /// it points at rather than the index it sits on, the way `active` does
    /// for projects — see [`Workspace::move_project`].
    pub fn move_space(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from == to || from >= self.spaces.len() || to >= self.spaces.len() {
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
        self.spaces = Self::in_order(store::all(), &order);
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
            .map(|member| member.project)
            .filter(|path| !self.projects.iter().any(|open| open.path == *path))
            .filter(|path| path.is_dir())
            .collect();
        for path in wanted {
            self.open_project(path, cx);
        }
        self.space = Some(ix);
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
                let space = store::create("", target.clone())?;
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
        store::save(space);
        self.space = Some(at);
        self.save();
        cx.notify();
        Some(at)
    }

    /// Put an arriving entry into the pane holding `target`, as a tab —
    /// a drop on a pane's bar rather than on its edge. There has to be a
    /// space already: a pane with one entry has no bar to drop on.
    pub fn stack_pane(&mut self, target: &Member, arriving: &Member, cx: &mut Context<Self>) {
        // An entry is in one space at a time, the same rule [`Self::arrange`]
        // follows for the same reason.
        let Some(keep) = self.active_space().map(|space| space.id.clone()) else {
            return;
        };
        self.evict_from_spaces(arriving, &keep, cx);
        self.space = self.spaces.iter().position(|space| space.id == keep);
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
            store::save(&mut self.spaces[ix]);
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
            .and_then(|member| Some((member.clone(), self.showing_of(&member)?)));

        if let Some(at) = self.space {
            self.delete_space(at, cx);
        }
        let (member, (project, showing)) = survivor?;
        self.select_showing(project, showing, cx);
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
            store::save(space);
            cx.notify();
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
        store::remove(&self.spaces.remove(ix).id);
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
        if let Some(ix) = self.spaces.iter().position(|space| space.id == id) {
            self.delete_space(ix, cx);
        }
    }

    pub fn rename_space(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        if let Some(space) = self.spaces.iter_mut().find(|space| space.id == id) {
            space.name = name.trim().to_owned();
            store::save(space);
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
            let path = open.path.clone();
            for chat in &open.sessions {
                if let Some(record) = chat.record.clone() {
                    live.push(Member::new(path.clone(), MemberKind::Session, record));
                }
            }
            for board in &open.boards {
                live.push(Member::new(
                    path.clone(),
                    MemberKind::Board,
                    board.id.clone(),
                ));
            }
            for article in &open.articles {
                live.push(Member::new(
                    path.clone(),
                    MemberKind::Article,
                    article.path.to_string_lossy().into_owned(),
                ));
            }
            for table in &open.tables {
                live.push(Member::new(
                    path.clone(),
                    MemberKind::Table,
                    table.key.clone(),
                ));
            }
        }
        let open: Vec<PathBuf> = self.projects.iter().map(|open| open.path.clone()).collect();
        for space in &mut self.spaces {
            let gone: Vec<Member> = space
                .entries()
                .into_iter()
                .filter(|member| open.contains(&member.project) && !live.contains(member))
                .collect();
            if !gone.is_empty() && space.prune(&gone) {
                store::save(space);
            }
        }
        cx.notify();
    }

    /// What the member names, and which open project it is in. Nothing for one
    /// whose project is shut, or which has gone since the space named it —
    /// that draws as an empty pane rather than a panic.
    pub fn showing_of(&self, member: &Member) -> Option<(usize, Showing)> {
        let at = self
            .projects
            .iter()
            .position(|open| open.path == member.project)?;
        let open = self.projects.get(at)?;
        let showing = match member.kind {
            MemberKind::Session => open
                .sessions
                .iter()
                .find(|chat| chat.record.as_deref() == Some(member.id.as_str()))
                .map(|chat| Showing::Session(chat.id))?,
            MemberKind::Board => open
                .boards
                .iter()
                .position(|board| board.id == member.id)
                .map(Showing::Board)?,
            MemberKind::Article => open
                .articles
                .iter()
                .position(|article| article.path.to_string_lossy() == member.id)
                .map(Showing::Article)?,
            MemberKind::Table => open
                .tables
                .iter()
                .position(|table| table.key == member.id)
                .map(Showing::Table)?,
        };
        Some((at, showing))
    }

    /// The member that names what a pane is on, so an entry opened from the
    /// sidebar can be put into a space.
    ///
    /// Nothing for a session with no file yet: a space names entries on disk,
    /// and one that has had no turn is not there to be named.
    pub fn member_of(&self, project: usize, showing: Showing) -> Option<Member> {
        let open = self.projects.get(project)?;
        let path = open.path.clone();
        Some(match showing {
            Showing::Session(id) => {
                Member::new(path, MemberKind::Session, open.session(id)?.record.clone()?)
            }
            Showing::Board(ix) => {
                Member::new(path, MemberKind::Board, open.boards.get(ix)?.id.clone())
            }
            Showing::Article(ix) => Member::new(
                path,
                MemberKind::Article,
                open.articles.get(ix)?.path.to_string_lossy().into_owned(),
            ),
            Showing::Table(ix) => {
                Member::new(path, MemberKind::Table, open.tables.get(ix)?.key.clone())
            }
        })
    }

    /// The board a pane is on, by its place in its project. Named apart from
    /// [`Self::board_at`], which finds one by file wherever it is open.
    pub fn board_in(&self, project: usize, ix: usize) -> Option<&Board> {
        if !self.settings.features.boards {
            return None;
        }
        self.projects.get(project)?.boards.get(ix)
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

    pub fn session_in(&self, project: usize, id: u64) -> Option<&ChatSession> {
        self.projects.get(project)?.session(id)
    }

    /// Put the project's selection on what a pane is showing.
    ///
    /// The four slots are what every command without a pane of its own reads,
    /// so this is what makes the focused pane the one they act on.
    ///
    /// Each kind is brought up to what drawing it needs, because putting a
    /// pane on an entry is the whole of how one arrives here — a drop, a tab
    /// coming forward, a space opening — and the `open_*` calls are only the
    /// sidebar's route. An article with no editor draws as the front door and
    /// an archived board draws as an empty one, so neither can be left to
    /// whoever asked.
    pub fn select_showing(&mut self, project: usize, showing: Showing, cx: &mut Context<Self>) {
        // Read before the project is borrowed for the rest of this.
        let text_size = self.article_font_size();
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        match showing {
            Showing::Session(id) => open.active = Some(id),
            Showing::Board(ix) => {
                if let Some(id) = open.boards.get(ix).map(|board| board.id.clone())
                    && open.load_board(&id)
                {
                    open.board = Some(ix);
                }
            }
            Showing::Article(ix) => {
                if let Some(article) = open.articles.get_mut(ix) {
                    article.open(text_size, cx);
                    open.article = Some(ix);
                }
            }
            Showing::Table(ix) => {
                open.table = Some(ix);
                open.reload_page();
            }
        }
        self.active = Some(project);
        if let Showing::Session(id) = showing {
            self.wake_session(id, cx);
        }
        cx.notify();
    }

    /// Change the open space and write it back, if the change took.
    fn edit_space(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Space) -> bool) {
        let Some(space) = self.space.and_then(|ix| self.spaces.get_mut(ix)) else {
            return;
        };
        if edit(space) {
            store::save(space);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/spaces.rs"]
mod space_tests;
