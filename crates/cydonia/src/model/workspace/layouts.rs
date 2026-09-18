//! The layouts a project holds: which entries are on screen together, and how
//! they are arranged.
//!
//! A continuation of [`Workspace`]'s one `impl`, the way [`super::boards`] is.
//!
//! A layout is made by dragging a second entry onto the one already open, so
//! there is no `new_layout` a menu calls — see [`Workspace::arrange`].
use super::*;
use artifact::layout::{Layout, Side};
use std::collections::HashSet;

/// What a pane shows, resolved from the [`crate::model::state::Kind`]-less
/// number a layout holds.
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
        let project = self.active_project()?;
        project.layouts.get(project.layout?)
    }

    pub fn active_layout_mut(&mut self) -> Option<&mut Layout> {
        let project = self.projects.get_mut(self.active?)?;
        project.layouts.get_mut(project.layout?)
    }

    /// Every project's layouts are on show, so picking one brings its project
    /// forward with it.
    pub fn open_layout(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= open.layouts.len() {
            return;
        }
        open.layout = Some(ix);
        let id = open.layouts[ix].id.clone();
        self.active = Some(project);
        self.remember(project, state::Kind::Layout, id, cx);
        cx.notify();
    }

    /// Put `arriving` beside `target`, making the layout that holds them if
    /// there is not one yet.
    ///
    /// This is the whole of how a layout is born: the first drag onto a pane
    /// mints one over the entry already there, and every drag after it lands
    /// in the same layout. Answers the index of the layout now open.
    pub fn arrange(
        &mut self,
        project: usize,
        target: u64,
        arriving: u64,
        side: Side,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let open = self.projects.get_mut(project)?;
        let at = match open.layout {
            // Already arranging: the drag lands in the layout on screen.
            Some(at) if at < open.layouts.len() => at,
            // The first drag. The layout starts as the pane that was there,
            // which is what the arrival is being put beside.
            _ => {
                let taken: HashSet<String> = open
                    .layouts
                    .iter()
                    .map(|layout| layout.name.clone())
                    .collect();
                let name = artifact::layout::next_name(&taken);
                let layout = open.store().create_layout(&name, target)?;
                open.layouts.insert(0, layout);
                0
            }
        };
        // An entry is in one layout at a time, the way a pane is in one tmux
        // window. Dragging it into another moves it: the sidebar lists it
        // under the layout that has it, and it can only be under one.
        let store = open.store();
        for (ix, other) in open.layouts.iter_mut().enumerate() {
            if ix != at && other.remove(arriving) {
                store.save_layout(other);
            }
        }
        let layout = open.layouts.get_mut(at)?;
        if !layout.insert(target, arriving, side) {
            return None;
        }
        store.save_layout(layout);
        let id = layout.id.clone();
        open.layout = Some(at);
        self.active = Some(project);
        self.remember(project, state::Kind::Layout, id, cx);
        cx.notify();
        Some(at)
    }

    /// Move a pane already on screen to another pane's edge.
    pub fn relocate_pane(&mut self, entry: u64, target: u64, side: Side, cx: &mut Context<Self>) {
        self.edit_layout(cx, |layout| layout.relocate(entry, target, side));
    }

    /// Close one pane.
    ///
    /// Closing the last one closes the layout: an arrangement of one pane is
    /// not an arrangement, and leaving the file behind would put a row in the
    /// sidebar for something the window is no longer doing. What that pane was
    /// on is left open on its own.
    pub fn close_pane(&mut self, entry: u64, cx: &mut Context<Self>) {
        let Some(project) = self.active else {
            return;
        };
        let last = self
            .active_layout()
            .is_some_and(|layout| layout.leaves() <= 1);
        if last {
            let at = self.projects.get(project).and_then(|open| open.layout);
            if let Some(at) = at {
                self.delete_layout(project, at, cx);
            }
            // The pane that was there stays open, now as the whole window.
            if let Some(showing) = self.showing_of(project, entry) {
                self.select_showing(project, showing, cx);
            }
            return;
        }
        self.edit_layout(cx, |layout| layout.remove(entry));
    }

    /// The pane across the seam on this side of the one in front, if there is
    /// one — what says whether a move that way is on offer at all.
    pub fn neighbour_pane(&self, entry: u64, side: Side) -> Option<u64> {
        self.active_layout()?.tree.neighbour(entry, side)
    }

    /// Exchange a pane with the one across the seam on that side. The
    /// arrangement keeps its shape and its sizes — see [`Node::swap`].
    pub fn move_pane(&mut self, entry: u64, side: Side, cx: &mut Context<Self>) -> bool {
        let Some(across) = self.neighbour_pane(entry, side) else {
            return false;
        };
        let mut moved = false;
        self.edit_layout(cx, |layout| {
            moved = layout.tree.swap(entry, across);
            moved
        });
        moved
    }

    /// Stand one pane over the others, or put it back.
    pub fn zoom_pane(&mut self, entry: u64, cx: &mut Context<Self>) {
        self.edit_layout(cx, |layout| {
            layout.zoom(entry);
            true
        });
    }

    /// Drop the layout: the file goes with it. The entries it arranged are
    /// left alone — a layout holds none of them.
    pub fn delete_layout(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= project.layouts.len() {
            return;
        }
        project
            .store()
            .remove_layout(&project.layouts.remove(ix).id);
        project.layout = project
            .layout
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        cx.notify();
    }

    /// Put the arrangement away, or bring it back. The members go with it —
    /// see [`Cydonia::archive_entry`], which walks them.
    pub fn archive_layout(
        &mut self,
        project: usize,
        ix: usize,
        archived: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let store = open.store();
        let Some(layout) = open.layouts.get_mut(ix) else {
            return;
        };
        layout.archived = archived;
        store.save_layout(layout);
        // An arrangement put away is not the one the window is showing.
        if archived && open.layout == Some(ix) {
            open.layout = None;
        }
        cx.notify();
    }

    pub fn rename_layout(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        for open in &mut self.projects {
            let store = open.store();
            if let Some(layout) = open.layouts.iter_mut().find(|layout| layout.id == id) {
                layout.name = name.trim().to_owned();
                store.save_layout(layout);
                cx.notify();
                return;
            }
        }
    }

    /// Drop members whose entries have gone. A layout names entries by number
    /// and holds none of them, so a session deleted from the sidebar leaves a
    /// pane pointing at nothing until this runs.
    pub fn prune_layouts(&mut self, cx: &mut Context<Self>) {
        for open in &mut self.projects {
            let live: HashSet<u64> = artifact::entry::list(&open.path)
                .unwrap_or_default()
                .into_iter()
                .map(|entry| entry.number)
                .collect();
            let store = open.store();
            for layout in &mut open.layouts {
                let gone: HashSet<u64> = layout
                    .entries()
                    .into_iter()
                    .filter(|entry| !live.contains(entry))
                    .collect();
                if !gone.is_empty() && layout.prune(&gone) {
                    store.save_layout(layout);
                }
            }
        }
        cx.notify();
    }

    /// What the entry with this number is, and where in the project to find
    /// it. Nothing for a number nothing answers to — an entry deleted since
    /// the layout named it, which draws as an empty pane rather than a panic.
    ///
    /// A scan of what is already loaded rather than a read of `entries.db`:
    /// every entry carries the number it was given, and this is asked once per
    /// pane per frame.
    pub fn showing_of(&self, project: usize, number: u64) -> Option<Showing> {
        let open = self.projects.get(project)?;
        if let Some(chat) = open
            .sessions
            .iter()
            .find(|chat| chat.number == Some(number))
        {
            return Some(Showing::Session(chat.id));
        }
        if let Some(ix) = open
            .boards
            .iter()
            .position(|board| board.number == Some(number))
        {
            return Some(Showing::Board(ix));
        }
        if let Some(ix) = open
            .articles
            .iter()
            .position(|article| article.number == Some(number))
        {
            return Some(Showing::Article(ix));
        }
        open.tables
            .iter()
            .position(|table| table.number == Some(number))
            .map(Showing::Table)
    }

    /// The board a pane is on, by its place in the project's list. What
    /// [`Self::active_board`] answers for the focused pane, asked for a pane
    /// that is not the focused one.
    pub fn board_at_ix(&self, ix: usize) -> Option<&Board> {
        if !self.settings.features.boards {
            return None;
        }
        self.active_project()?.boards.get(ix)
    }

    pub fn article_at_ix(&self, ix: usize) -> Option<&Article> {
        self.active_project()?.articles.get(ix)
    }

    pub fn table_at_ix(&self, ix: usize) -> Option<&Table> {
        if !self.settings.features.tables {
            return None;
        }
        self.active_project()?.tables.get(ix)
    }

    /// The rows a table pane is on, by the table's place in the project.
    pub fn page_at_ix(&self, ix: usize) -> Option<&Page> {
        if !self.settings.features.tables {
            return None;
        }
        let open = self.active_project()?;
        open.pages.get(&open.tables.get(ix)?.key)
    }

    pub fn session_by_id(&self, id: u64) -> Option<&ChatSession> {
        self.active_project()?
            .sessions
            .iter()
            .find(|chat| chat.id == id)
    }

    /// The layout a given entry is a member of, if any — what the sidebar
    /// lists it under.
    pub fn layout_holding(&self, project: usize, number: u64) -> Option<usize> {
        self.projects
            .get(project)?
            .layouts
            .iter()
            .position(|layout| layout.contains(number))
    }

    /// Put the project's selection on what a pane is showing.
    ///
    /// The four slots are what every command without a pane of its own reads,
    /// so this is what makes the focused pane the one they act on. A session
    /// is woken, the way landing on one is — see [`Self::wake_session`].
    pub fn select_showing(&mut self, project: usize, showing: Showing, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        match showing {
            Showing::Session(id) => open.active = Some(id),
            Showing::Board(ix) => open.board = Some(ix),
            Showing::Article(ix) => open.article = Some(ix),
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

    /// The number the entry a pane is on carries, so a pane opened from the
    /// sidebar can be put into a layout.
    pub fn number_of(&self, project: usize, showing: Showing) -> Option<u64> {
        let open = self.projects.get(project)?;
        match showing {
            Showing::Session(id) => open
                .sessions
                .iter()
                .find(|chat| chat.id == id)
                .and_then(|chat| chat.number),
            Showing::Board(ix) => open.boards.get(ix).and_then(|board| board.number),
            Showing::Article(ix) => open.articles.get(ix).and_then(|article| article.number),
            Showing::Table(ix) => open.tables.get(ix).and_then(|table| table.number),
        }
    }

    /// Change the open layout and write it back, if the change took.
    fn edit_layout(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Layout) -> bool) {
        let Some(open) = self.active.and_then(|ix| self.projects.get_mut(ix)) else {
            return;
        };
        let store = open.store();
        let Some(layout) = open.layout.and_then(|ix| open.layouts.get_mut(ix)) else {
            return;
        };
        if edit(layout) {
            store.save_layout(layout);
            cx.notify();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/layouts.rs"]
mod layout_tests;
