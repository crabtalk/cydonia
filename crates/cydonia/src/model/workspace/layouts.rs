//! The layouts this machine holds: which entries are on screen together, and
//! how they are arranged.
//!
//! A continuation of [`Workspace`]'s one `impl`, the way [`super::boards`] is.
//!
//! A layout is not a project's. It names entries by the project they are in
//! and the id they have there, so one arrangement can hold a session from one
//! repository beside a board from another — which is why the window owns them
//! and why they are kept beside the config rather than in any `.cydonia/`. See
//! [`crate::model::layouts`] for where they are written.
//!
//! There is no `new_layout` a menu calls: a layout is made by dragging a
//! second entry onto the one already open — see [`Workspace::arrange`].
use super::*;
use crate::model::layouts as store;
use artifact::layout::{Kind as MemberKind, Layout, Member, Side};

/// What a pane shows, resolved from the member a layout names.
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
    /// The layout the window is arranged by, if one is open.
    pub fn active_layout(&self) -> Option<&Layout> {
        self.layouts.get(self.layout?)
    }

    pub fn active_layout_mut(&mut self) -> Option<&mut Layout> {
        self.layouts.get_mut(self.layout?)
    }

    /// Read every layout back off disk. Called at launch, since nothing else
    /// writes them.
    pub fn reload_layouts(&mut self, cx: &mut Context<Self>) {
        let open = self.active_layout().map(|layout| layout.id.clone());
        self.layouts = store::all();
        self.layout = open.and_then(|id| self.layouts.iter().position(|at| at.id == id));
        cx.notify();
    }

    /// Show a layout. The projects its panes are in are opened if they are not
    /// already — an arrangement is a working context, and half of one is not
    /// what was asked for.
    pub fn open_layout(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(layout) = self.layouts.get(ix) else {
            return;
        };
        let wanted: Vec<PathBuf> = layout
            .entries()
            .into_iter()
            .map(|member| member.project)
            .filter(|path| !self.projects.iter().any(|open| open.path == *path))
            .filter(|path| path.is_dir())
            .collect();
        for path in wanted {
            self.open_project(path, cx);
        }
        self.layout = Some(ix);
        self.save();
        cx.notify();
    }

    /// Leave the arrangement, which is what opening any single entry does.
    pub fn leave_layout(&mut self) {
        self.layout = None;
    }

    /// Put `arriving` beside `target`, making the layout that holds them if
    /// there is not one yet.
    ///
    /// This is the whole of how a layout is born: the first drag onto a pane
    /// mints one over the entry already there, and every drag after it lands
    /// in the same layout.
    pub fn arrange(
        &mut self,
        target: &Member,
        arriving: &Member,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let at = match self.layout {
            Some(at) if at < self.layouts.len() => at,
            // The first drag. The layout starts as the pane that was there,
            // which is what the arrival is being put beside.
            _ => {
                let layout = store::create("", target.clone())?;
                self.layouts.insert(0, layout);
                0
            }
        };
        // An entry is in one layout at a time, the way a pane is in one tmux
        // window. Dragging it into another moves it: the sidebar lists it
        // under the layout that has it, and it can only be under one.
        for (ix, other) in self.layouts.iter_mut().enumerate() {
            if ix != at && other.remove(arriving) {
                store::save(other);
            }
        }
        let layout = self.layouts.get_mut(at)?;
        if !layout.insert(target, arriving, side) {
            return None;
        }
        store::save(layout);
        self.layout = Some(at);
        self.save();
        cx.notify();
        Some(at)
    }

    /// Put an arriving entry into the pane holding `target`, as a tab —
    /// a drop on a pane's bar rather than on its edge. There has to be a
    /// layout already: a pane with one entry has no bar to drop on.
    pub fn stack_pane(&mut self, target: &Member, arriving: &Member, cx: &mut Context<Self>) {
        // An entry is in one layout at a time, the same rule [`Self::arrange`]
        // follows for the same reason.
        let Some(at) = self.layout else {
            return;
        };
        for (ix, other) in self.layouts.iter_mut().enumerate() {
            if ix != at && other.remove(arriving) {
                store::save(other);
            }
        }
        self.edit_layout(cx, |layout| layout.stack(target, arriving));
    }

    /// The entries the pane holding this one draws, in strip order. One entry
    /// for a pane holding one thing, and none at all where no layout is open.
    pub fn stack_of(&self, entry: &Member) -> Vec<Member> {
        self.active_layout()
            .map(|layout| layout.stack_of(entry))
            .unwrap_or_default()
    }

    /// The pane across the seam on this side of the one given, if there is one
    /// — what says whether a move that way is on offer at all.
    pub fn neighbour_pane(&self, entry: &Member, side: Side) -> Option<Member> {
        self.active_layout()?.tree.neighbour(entry, side)
    }

    /// Exchange a pane with the one across the seam on that side. The
    /// arrangement keeps its shape and its sizes.
    pub fn move_pane(&mut self, entry: &Member, side: Side, cx: &mut Context<Self>) -> bool {
        let Some(across) = self.neighbour_pane(entry, side) else {
            return false;
        };
        let mut moved = false;
        self.edit_layout(cx, |layout| {
            moved = layout.tree.swap(entry, &across);
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
        self.edit_layout(cx, |layout| layout.relocate(entry, target, side));
    }

    /// Close one pane.
    ///
    /// Closing down to one pane closes the layout: an arrangement of one pane
    /// is not an arrangement, and leaving the file behind would put a row in
    /// the sidebar for something the window is no longer doing. The pane that
    /// would have been left alone is what the window is put on, so the entry
    /// you were keeping stays in front — and is returned, so the caller can
    /// put the single pane on its kind. Nothing while the arrangement
    /// survives, which leaves the panes to say what they show.
    pub fn close_pane(&mut self, entry: &Member, cx: &mut Context<Self>) -> Option<Showing> {
        let layout = self.active_layout()?;
        // A pane holding tabs loses a tab, not the pane — so none of the rules
        // below about what is left of the arrangement come into it.
        if layout.stack_of(entry).len() > 1 || layout.leaves() > 2 {
            self.edit_layout(cx, |layout| layout.remove(entry));
            return None;
        }
        let survivor = layout
            .entries()
            .into_iter()
            .find(|member| member != entry)
            .and_then(|member| self.showing_of(&member));

        if let Some(at) = self.layout {
            self.delete_layout(at, cx);
        }
        let (project, showing) = survivor?;
        self.select_showing(project, showing, cx);
        Some(showing)
    }

    /// Take an entry out of whatever layout holds it, open or not.
    ///
    /// [`Self::close_pane`] is the gesture's version of this and works on the
    /// layout in front; this one is for an entry that is going away from the
    /// list altogether, which can be holding a pane in an arrangement nobody
    /// is looking at. Taking the last pane out takes the layout with it, the
    /// same rule and for the same reason.
    pub fn drop_from_layouts(&mut self, member: &Member, cx: &mut Context<Self>) {
        let Some(ix) = self.layout_holding(member) else {
            return;
        };
        let Some(layout) = self.layouts.get_mut(ix) else {
            return;
        };
        // A pane holding tabs loses a tab and stays a pane, so what is left of
        // the arrangement does not change.
        if layout.leaves() <= 2 && layout.stack_of(member).len() <= 1 {
            self.delete_layout(ix, cx);
            return;
        }
        if layout.remove(member) {
            store::save(layout);
            cx.notify();
        }
    }

    /// Stand one pane over the others, or put it back.
    pub fn zoom_pane(&mut self, entry: &Member, cx: &mut Context<Self>) {
        self.edit_layout(cx, |layout| {
            layout.zoom(entry);
            true
        });
    }

    /// Drop the layout: the file goes with it. The entries it arranged are
    /// left alone — a layout holds none of them.
    pub fn delete_layout(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix >= self.layouts.len() {
            return;
        }
        store::remove(&self.layouts.remove(ix).id);
        self.layout = self
            .layout
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        self.save();
        cx.notify();
    }

    /// Put the arrangement away, or bring it back. The members go with it —
    /// see `Cydonia::archive_entry`, which walks them.
    pub fn archive_layout(&mut self, ix: usize, archived: bool, cx: &mut Context<Self>) {
        let Some(layout) = self.layouts.get_mut(ix) else {
            return;
        };
        layout.archived = archived;
        store::save(layout);
        // An arrangement put away is not the one the window is showing.
        if archived && self.layout == Some(ix) {
            self.layout = None;
        }
        self.save();
        cx.notify();
    }

    pub fn rename_layout(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        if let Some(layout) = self.layouts.iter_mut().find(|layout| layout.id == id) {
            layout.name = name.trim().to_owned();
            store::save(layout);
            cx.notify();
        }
    }

    /// The layout a given entry is a member of, if any — what the sidebar
    /// lists it under. One put away holds nothing: its members are listed
    /// where they would be without it.
    pub fn layout_holding(&self, member: &Member) -> Option<usize> {
        self.layouts
            .iter()
            .position(|layout| !layout.archived && layout.contains(member))
    }

    /// Drop members whose entries have gone. A layout names entries and holds
    /// none of them, so one deleted from the sidebar leaves a pane pointing at
    /// nothing until this runs.
    ///
    /// Only for projects that are open: a member in a project this window has
    /// never opened is not missing, only out of reach.
    pub fn prune_layouts(&mut self, cx: &mut Context<Self>) {
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
        for layout in &mut self.layouts {
            let gone: Vec<Member> = layout
                .entries()
                .into_iter()
                .filter(|member| open.contains(&member.project) && !live.contains(member))
                .collect();
            if !gone.is_empty() && layout.prune(&gone) {
                store::save(layout);
            }
        }
        cx.notify();
    }

    /// What the member names, and which open project it is in. Nothing for one
    /// whose project is shut, or which has gone since the layout named it —
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
    /// sidebar can be put into a layout.
    ///
    /// Nothing for a session with no file yet: a layout names entries on disk,
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
    /// coming forward, a layout opening — and the `open_*` calls are only the
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

    /// Change the open layout and write it back, if the change took.
    fn edit_layout(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Layout) -> bool) {
        let Some(layout) = self.layout.and_then(|ix| self.layouts.get_mut(ix)) else {
            return;
        };
        if edit(layout) {
            store::save(layout);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/layouts.rs"]
mod layout_tests;
