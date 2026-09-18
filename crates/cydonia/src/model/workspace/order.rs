//! The order a project's entries are listed in. The sidebar only moves a row
//! when it is dragged; nothing a write does reorders the list.
//!
//! The list is held by project path and written to `state.toml`, beside the
//! entry each project was last showing. Not in the project's own `.cydonia/`:
//! an arrangement is what one person likes looking at, the way a layout is,
//! and neither is the project's source — see [`crate::model::layouts`].
//!
//! Identity here is the pair `state.toml` already keeps — a session's record,
//! a board's id, an article's path, a table's key — never an index, which
//! moves the moment a neighbour is made or dropped.

use super::{Showing, Workspace};
use crate::model::state;
use artifact::layout::Kind as MemberKind;
use bezel::gpui::Context;

impl Workspace {
    /// What `state.toml` calls the entry a pane would be put on.
    ///
    /// `None` for a session with no file yet: the order is written down, and a
    /// session that has had no turn has no name to write.
    pub fn entry_of(&self, project: usize, showing: Showing) -> Option<state::Entry> {
        let member = self.member_of(project, showing)?;
        let kind = match member.kind {
            MemberKind::Session => state::Kind::Session,
            MemberKind::Board => state::Kind::Board,
            MemberKind::Article => state::Kind::Article,
            MemberKind::Table => state::Kind::Table,
        };
        Some(state::Entry {
            kind,
            id: member.id,
        })
    }

    /// Where an entry sits in the order its project was left in.
    ///
    /// `None` is an entry made since the order was written — a new one, or the
    /// first look at a project nobody has dragged a row in. The sidebar lists
    /// those above the arrangement rather than below it: a new entry that
    /// appeared under the fold would be one nobody saw arrive.
    pub fn rank_of(&self, project: usize, showing: Showing) -> Option<usize> {
        let path = &self.projects.get(project)?.path;
        let entry = self.entry_of(project, showing)?;
        self.order
            .get(path)?
            .iter()
            .position(|held| held.kind == entry.kind && held.id == entry.id)
    }

    /// Where an entry sits among the pins, and `None` for one that is not
    /// pinned. What puts the pinned rows above the rest, in an order of their
    /// own.
    pub fn pin_rank(&self, project: usize, showing: Showing) -> Option<usize> {
        let path = &self.projects.get(project)?.path;
        let entry = self.entry_of(project, showing)?;
        self.pinned
            .get(path)?
            .iter()
            .position(|held| held.kind == entry.kind && held.id == entry.id)
    }

    pub fn is_pinned(&self, project: usize, showing: Showing) -> bool {
        self.pin_rank(project, showing).is_some()
    }

    /// Pin an entry to the top of its project's list, or let it back down.
    ///
    /// A new pin goes to the foot of the pins rather than the head: the row
    /// that was already at the top is the one somebody is used to reaching
    /// for, and a pin that displaced it would move the list it was meant to
    /// hold still.
    pub fn pin(&mut self, project: usize, showing: Showing, on: bool, cx: &mut Context<Self>) {
        let Some(entry) = self.entry_of(project, showing) else {
            return;
        };
        let Some(open) = self.projects.get(project) else {
            return;
        };
        let held = self.pinned.entry(open.path.clone()).or_default();
        held.retain(|pin| !(pin.kind == entry.kind && pin.id == entry.id));
        if on {
            held.push(entry);
        }
        self.save();
        cx.notify();
    }

    /// Drop an entry's pin without touching anything else — what archiving one
    /// does on the way past. Quiet about an entry that was never pinned.
    pub fn unpin_entry(&mut self, project: usize, showing: Showing) {
        let Some(entry) = self.entry_of(project, showing) else {
            return;
        };
        let Some(open) = self.projects.get(project) else {
            return;
        };
        if let Some(held) = self.pinned.get_mut(&open.path) {
            held.retain(|pin| !(pin.kind == entry.kind && pin.id == entry.id));
        }
        self.save();
    }

    /// Write down the order a project's rows are now in. The sidebar hands the
    /// whole list rather than the one row that moved: what the rest of them
    /// are is a question only the list on screen can answer, and it is holding
    /// the answer already.
    pub fn set_order(&mut self, project: usize, order: Vec<state::Entry>, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get(project) else {
            return;
        };
        self.order.insert(open.path.clone(), order);
        self.save();
        cx.notify();
    }

    /// Write down which of a project's entries are pinned, and in what order.
    pub fn set_pinned(
        &mut self,
        project: usize,
        pinned: Vec<state::Entry>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.projects.get(project) else {
            return;
        };
        self.pinned.insert(open.path.clone(), pinned);
        self.save();
        cx.notify();
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/entry_order.rs"]
mod order_tests;
