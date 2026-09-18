//! A project's layouts: several entries on screen at once, and how they are
//! arranged.
//!
//! One file per layout under `.cydonia/layouts/`, named for the millisecond it
//! was made, the way boards are.
//!
//! A layout holds no entry. It holds the [`crate::entry`] numbers of entries
//! that exist beside it, so the same session can be a member of two layouts
//! and stay one session. A number outlives a rename — see
//! [`crate::entry::Registry::rename`] — and a removed entry leaves its number
//! behind as a tombstone, so a member that has been deleted resolves to
//! nothing rather than to whatever was filed next.
//!
//! The tree nests: children are a `Vec`, so a third pane alongside two others
//! joins their split rather than nesting inside one of them.

use crate::{id, stamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// What a layout is shown as before it is named.
pub const UNNAMED: &str = "Untitled";

/// The stem every auto-given name is built on: `layout-1`, `layout-2`.
pub const STEM: &str = "layout";

/// Which way a split divides the room it is given.
///
/// Its own rather than gpui's: nothing in this crate names a window — see the
/// crate docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Children side by side, dividing the width.
    Horizontal,
    /// Children stacked, dividing the height.
    Vertical,
}

/// Which edge of a pane something is dropped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    /// The axis a split made at this edge divides along.
    pub fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Above | Self::Below => Axis::Vertical,
        }
    }

    /// Whether the arrival goes after the pane it was dropped on.
    fn after(self) -> bool {
        matches!(self, Self::Right | Self::Below)
    }

    /// The side facing this one.
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Above => Self::Below,
            Self::Below => Self::Above,
        }
    }
}

/// One pane, or one split of them.
///
/// Every node carries its own share of the parent rather than the parent
/// holding a list of shares beside a list of children: two lists are two
/// things to keep the same length, and a file hand-edited into disagreeing
/// them has no obvious reading.
///
/// `f64` for the share, where the view works in `f32`. TOML has one float and
/// it is this one, and an `f32` widened on the way out writes `0.3` as
/// `0.30000001192092896`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    /// An entry, by the number it holds project-wide. The entry it names may
    /// since have been deleted; resolving is [`crate::entry::Registry`]'s and
    /// answers nothing for one that has.
    Leaf {
        #[serde(default = "whole")]
        ratio: f64,
        entry: u64,
    },
    Split {
        #[serde(default = "whole")]
        ratio: f64,
        axis: Axis,
        children: Vec<Node>,
    },
}

fn whole() -> f64 {
    1.
}

/// A leaf taking half of whatever it is put in — what a pane divided in two
/// leaves on each side of the new seam.
fn half(entry: u64) -> Node {
    Node::Leaf { ratio: 0.5, entry }
}

impl Node {
    pub fn leaf(entry: u64) -> Self {
        Self::Leaf {
            ratio: whole(),
            entry,
        }
    }

    pub fn split(axis: Axis, children: Vec<Node>) -> Self {
        Self::Split {
            ratio: whole(),
            axis,
            children,
        }
    }

    /// This node's share of the parent's extent along the parent's axis. The
    /// root's is the whole of it.
    ///
    /// A share rather than a width in pixels: the window is resized between
    /// one launch and the next, and a layout written in pixels would come back
    /// either overflowing it or leaving a strip of it empty.
    pub fn ratio(&self) -> f64 {
        match self {
            Self::Leaf { ratio, .. } | Self::Split { ratio, .. } => *ratio,
        }
    }

    pub fn set_ratio(&mut self, to: f64) {
        match self {
            Self::Leaf { ratio, .. } | Self::Split { ratio, .. } => *ratio = to,
        }
    }

    /// Every entry number below here, in the order the panes are laid out.
    pub fn entries(&self) -> Vec<u64> {
        match self {
            Self::Leaf { entry, .. } => vec![*entry],
            Self::Split { children, .. } => children.iter().flat_map(Node::entries).collect(),
        }
    }

    /// How many panes this draws.
    pub fn leaves(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { children, .. } => children.iter().map(Node::leaves).sum(),
        }
    }

    /// Share the parent's extent evenly among this node's children, and among
    /// theirs. What a split is left at when a pane is added or taken away, in
    /// place of whatever the ratios summed to once one of them went.
    pub fn even(&mut self) {
        let Self::Split { children, .. } = self else {
            return;
        };
        if children.is_empty() {
            return;
        }
        let share = 1. / children.len() as f64;
        for child in children.iter_mut() {
            child.set_ratio(share);
            child.even();
        }
    }

    /// Drop every leaf holding one of `gone`, and any split left empty by it.
    /// Answers whether anything went.
    ///
    /// A split down to one child is replaced by that child: a division with
    /// nothing on one side of it is a divider the pointer can still catch.
    ///
    /// What the departed held is shared out among what is left, so a split
    /// still covers the room it was given.
    pub fn prune(&mut self, gone: &HashSet<u64>) -> bool {
        let Self::Split { children, .. } = self else {
            return false;
        };
        let before = children.len();
        let mut changed = false;
        for child in children.iter_mut() {
            changed |= child.prune(gone);
        }
        children.retain(|child| match child {
            Self::Leaf { entry, .. } => !gone.contains(entry),
            Self::Split { children, .. } => !children.is_empty(),
        });
        let left = children.len() != before;
        changed |= left;
        if children.len() == 1 {
            let only = children.remove(0);
            let ratio = self.ratio();
            *self = only;
            self.set_ratio(ratio);
            return true;
        }
        if left {
            self.normalize();
        }
        changed
    }

    /// Scale this split's children so their shares sum to the whole. Shares
    /// that sum to anything else leave the last pane clipped or a strip of the
    /// window bare.
    fn normalize(&mut self) {
        let Self::Split { children, .. } = self else {
            return;
        };
        let total: f64 = children.iter().map(Node::ratio).sum();
        if children.is_empty() {
            return;
        }
        // Nothing to scale from — every share is nought, so they share alike.
        if total <= f64::EPSILON {
            self.even();
            return;
        }
        for child in children.iter_mut() {
            let scaled = child.ratio() / total;
            child.set_ratio(scaled);
        }
    }

    /// Whether some pane here shows this entry.
    pub fn contains(&self, entry: u64) -> bool {
        match self {
            Self::Leaf { entry: held, .. } => *held == entry,
            Self::Split { children, .. } => children.iter().any(|child| child.contains(entry)),
        }
    }

    /// Put `arriving` beside the pane showing `target`, on the given side.
    /// Answers whether `target` was found.
    ///
    /// A pane dropped on the edge of a split that already divides that way
    /// joins it as another child rather than nesting inside its neighbour —
    /// three side by side is one split of three. The room for the arrival
    /// comes out of the pane it was dropped on, so the others keep the shares
    /// they were dragged to.
    pub fn insert(&mut self, target: u64, arriving: u64, side: Side) -> bool {
        if let Self::Leaf { entry, ratio } = self
            && *entry == target
        {
            let (kept, ratio) = (*entry, *ratio);
            // The arrival takes the side it was dropped on, so the pane that
            // was already there does not jump across the new seam.
            let children = match side.after() {
                true => vec![half(kept), half(arriving)],
                false => vec![half(arriving), half(kept)],
            };
            *self = Self::Split {
                ratio,
                axis: side.axis(),
                children,
            };
            return true;
        }
        let Self::Split { axis, children, .. } = self else {
            return false;
        };
        let axis = *axis;
        let at = children.iter().position(|child| match child {
            Self::Leaf { entry, .. } => *entry == target,
            Self::Split { .. } => false,
        });
        if let Some(at) = at {
            if axis == side.axis() {
                // Halve what the dropped-on pane holds and hand the arrival
                // the other half: every other child keeps its share.
                let share = children[at].ratio() / 2.;
                children[at].set_ratio(share);
                let mut arrived = Node::leaf(arriving);
                arrived.set_ratio(share);
                children.insert(at + usize::from(side.after()), arrived);
                return true;
            }
            // Across the split's own direction, so the pane divides in place.
            return children[at].insert(target, arriving, side);
        }
        children
            .iter_mut()
            .any(|child| child.insert(target, arriving, side))
    }

    /// How much of the arrangement's width, or height, the pane on this entry
    /// takes: the shares of every split above it that divides along `axis`,
    /// multiplied together.
    ///
    /// A split the other way does not narrow it — two panes stacked are each
    /// as wide as the column holding them. Nothing for an entry no pane here
    /// is on.
    ///
    /// What a pane needs to know to draw itself as a pane rather than as the
    /// window: a rail, a margin or a column that has room in the window may
    /// have none in the fraction of it this pane is.
    pub fn share_of(&self, entry: u64, axis: Axis) -> Option<f64> {
        match self {
            Self::Leaf { entry: held, .. } => (*held == entry).then_some(1.),
            Self::Split {
                axis: split,
                children,
                ..
            } => children.iter().find_map(|child| {
                let share = child.share_of(entry, axis)?;
                Some(match *split == axis {
                    true => share * child.ratio(),
                    false => share,
                })
            }),
        }
    }

    /// Take the pane showing this entry out, collapsing whatever it leaves
    /// behind. Answers whether it was there.
    pub fn remove(&mut self, entry: u64) -> bool {
        if !self.contains(entry) {
            return false;
        }
        self.prune(&HashSet::from([entry]))
    }

    /// The node at this path: each step is a child's index in the split
    /// above it, so an empty path is this node.
    pub fn at_path_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let Some((step, rest)) = path.split_first() else {
            return Some(self);
        };
        let Self::Split { children, .. } = self else {
            return None;
        };
        children.get_mut(*step)?.at_path_mut(rest)
    }

    /// The path from here to the pane on this entry: each step is a child's
    /// index in the split above it.
    pub fn path_to(&self, entry: u64) -> Option<Vec<usize>> {
        match self {
            Self::Leaf { entry: held, .. } => (*held == entry).then(Vec::new),
            Self::Split { children, .. } => children.iter().enumerate().find_map(|(ix, child)| {
                let mut path = child.path_to(entry)?;
                path.insert(0, ix);
                Some(path)
            }),
        }
    }

    /// The node at this path — see [`Self::at_path_mut`].
    pub fn at_path(&self, path: &[usize]) -> Option<&Node> {
        let Some((step, rest)) = path.split_first() else {
            return Some(self);
        };
        let Self::Split { children, .. } = self else {
            return None;
        };
        children.get(*step)?.at_path(rest)
    }

    /// The pane nearest this side of whatever is under here — what a walk
    /// across a seam lands on when the other side of it is itself divided.
    ///
    /// Nothing for a split holding no children, which a well-formed tree never
    /// has — see [`Self::prune`], which collapses one the moment it could.
    fn edge_leaf(&self, side: Side) -> Option<u64> {
        match self {
            Self::Leaf { entry, .. } => Some(*entry),
            Self::Split { axis, children, .. } => {
                // Along this side's own axis the nearest pane is the one at
                // that end; across it every child touches the seam, and the
                // first is as good an answer as any.
                let at = match *axis == side.axis() && side.after() {
                    true => children.len().checked_sub(1)?,
                    false => 0,
                };
                children.get(at)?.edge_leaf(side)
            }
        }
    }

    /// The pane across the seam on this side of the one on `entry`, or nothing
    /// where that side of it is the edge of the window.
    ///
    /// The walk tiling window managers use, and the same one `select-pane -L`
    /// makes in tmux: up to the nearest split dividing the right way where
    /// this is not already the end child, across to the sibling, then down to
    /// whichever of its panes touches the seam.
    pub fn neighbour(&self, entry: u64, side: Side) -> Option<u64> {
        let path = self.path_to(entry)?;
        for depth in (0..path.len()).rev() {
            let Some(Self::Split { axis, children, .. }) = self.at_path(&path[..depth]) else {
                continue;
            };
            if *axis != side.axis() {
                continue;
            }
            let at = path[depth];
            let across = match side.after() {
                true => at + 1,
                false => match at {
                    // The end child on this side; the answer, if there is
                    // one, is further up.
                    0 => continue,
                    _ => at - 1,
                },
            };
            if let Some(sibling) = children.get(across) {
                // Entering from the far side, so the pane wanted is the one
                // against the seam just crossed.
                return sibling.edge_leaf(side.opposite());
            }
        }
        None
    }

    /// Exchange the places of two panes. The arrangement keeps its shape and
    /// its sizes; only what each pane is on changes — so doing it twice puts
    /// everything back.
    pub fn swap(&mut self, a: u64, b: u64) -> bool {
        if a == b || !self.contains(a) || !self.contains(b) {
            return false;
        }
        self.replace_entry(a, b);
        true
    }

    /// Put `b` where `a` is and `a` where `b` is.
    fn replace_entry(&mut self, a: u64, b: u64) {
        match self {
            Self::Leaf { entry, .. } => {
                if *entry == a {
                    *entry = b;
                } else if *entry == b {
                    *entry = a;
                }
            }
            Self::Split { children, .. } => {
                for child in children.iter_mut() {
                    child.replace_entry(a, b);
                }
            }
        }
    }

    /// Move the seam after child `at` so that everything before it takes
    /// `fraction` of this split. Answers whether it moved.
    ///
    /// Only the two children either side of the seam change: a drag moves one
    /// division, and the panes further along keep the widths they were put at.
    /// Neither may go below `min`, so a seam pushed past a neighbour stops
    /// rather than closing it — a pane with no width is one nothing can grab
    /// to bring back.
    pub fn resize(&mut self, at: usize, fraction: f64, min: f64) -> bool {
        let Self::Split { children, .. } = self else {
            return false;
        };
        if at + 1 >= children.len() {
            return false;
        }
        let before: f64 = children.iter().take(at).map(Node::ratio).sum();
        let pair = children[at].ratio() + children[at + 1].ratio();
        // Where the seam may sit: `min` inside each of the two it divides.
        let low = before + min;
        let high = before + pair - min;
        if high < low {
            return false;
        }
        let at_fraction = fraction.clamp(low, high);
        let first = at_fraction - before;
        if (first - children[at].ratio()).abs() < f64::EPSILON {
            return false;
        }
        children[at].set_ratio(first);
        children[at + 1].set_ratio(pair - first);
        true
    }

    /// Move a pane already here to another pane's edge.
    ///
    /// Taken out before it is put back, so the split it leaves collapses the
    /// way it would have if the pane had been closed — a pane dragged out of a
    /// pair does not leave the seam it was holding.
    pub fn relocate(&mut self, entry: u64, target: u64, side: Side) -> bool {
        if entry == target || !self.contains(entry) || !self.contains(target) {
            return false;
        }
        self.remove(entry);
        self.insert(target, entry, side)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    /// What names this layout for as long as it exists, and what its file is
    /// called. Written into the file as well, the way a board's is: a backend
    /// that keeps layouts in a row has no filename to fall back on.
    #[serde(default)]
    pub id: String,
    #[serde(skip)]
    pub number: Option<u64>,
    /// When it was last written, as the backend counts. Never written into the
    /// file, which would be a second copy able to disagree.
    #[serde(skip)]
    pub touched: u128,
    #[serde(default)]
    pub archived: bool,
    /// Given by [`next_name`] when the layout is made, and replaced by
    /// whatever it is renamed to.
    #[serde(default)]
    pub name: String,
    /// The pane standing over the others, if one is. The tree keeps its shape
    /// while it does: unzooming puts every pane back where it was, at the size
    /// it was.
    ///
    /// Before `tree` in the struct, and so in the file: `tree` is a table, and
    /// TOML reads a bare key after one as belonging to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed: Option<u64>,
    pub tree: Node,
}

impl Layout {
    /// A layout over one entry. Layouts are made by dragging a second entry
    /// onto the first, so the one already open is what a new one starts from.
    pub fn new(id: String, name: &str, entry: u64) -> Self {
        Self {
            id,
            number: None,
            touched: stamp::now(),
            archived: false,
            name: name.to_owned(),
            zoomed: None,
            tree: Node::leaf(entry),
        }
    }

    /// The sidebar's label.
    pub fn label(&self) -> &str {
        match self.name.is_empty() {
            true => UNNAMED,
            false => &self.name,
        }
    }

    pub fn entries(&self) -> Vec<u64> {
        self.tree.entries()
    }

    pub fn leaves(&self) -> usize {
        self.tree.leaves()
    }

    /// Drop members that no longer resolve — see [`Node::prune`].
    pub fn prune(&mut self, gone: &HashSet<u64>) -> bool {
        self.tree.prune(gone)
    }

    pub fn contains(&self, entry: u64) -> bool {
        self.tree.contains(entry)
    }

    /// Put an entry beside one already here — a drag from the sidebar onto a
    /// pane's edge. Answers whether the pane dropped on was found.
    ///
    /// An entry already shown is moved rather than shown twice.
    pub fn insert(&mut self, target: u64, arriving: u64, side: Side) -> bool {
        match self.contains(arriving) {
            true => self.tree.relocate(arriving, target, side),
            false => self.tree.insert(target, arriving, side),
        }
    }

    /// Move a pane already here to another pane's edge — see
    /// [`Node::relocate`].
    pub fn relocate(&mut self, entry: u64, target: u64, side: Side) -> bool {
        self.tree.relocate(entry, target, side)
    }

    /// Close a pane. The layout stays when its last pane goes: it is deleted
    /// from the sidebar and nowhere else.
    ///
    /// A zoomed pane that is closed leaves the rest unzoomed rather than
    /// standing something else in its place.
    pub fn remove(&mut self, entry: u64) -> bool {
        if self.zoomed == Some(entry) {
            self.zoomed = None;
        }
        self.tree.remove(entry)
    }

    /// Stand one pane over the others, or put it back. Answers what is zoomed
    /// afterwards.
    ///
    /// Zooming the pane already zoomed unzooms it, and zooming another swaps
    /// to it — one pane is over the rest, or none is.
    pub fn zoom(&mut self, entry: u64) -> Option<u64> {
        self.zoomed = match self.zoomed == Some(entry) || !self.contains(entry) {
            true => None,
            false => Some(entry),
        };
        self.zoomed
    }

    /// The pane standing over the others, if it is still here. Read rather
    /// than the field: a member pruned away must not leave the layout showing
    /// a pane that has gone.
    pub fn zoomed(&self) -> Option<u64> {
        self.zoomed.filter(|entry| self.contains(*entry))
    }
}

/// The name a new layout takes: `layout-1`, then `layout-2`.
///
/// Counted above the highest taken rather than into the first gap. A name is
/// how a layout is asked for, and one reused after a delete would answer for a
/// layout the asker never saw.
pub fn next_name(taken: &HashSet<String>) -> String {
    let highest = taken
        .iter()
        .filter_map(|name| {
            name.strip_prefix(STEM)?
                .strip_prefix('-')?
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0);
    format!("{STEM}-{}", highest + 1)
}

/// An id for a layout a backend has no file to name one from.
pub fn mint() -> String {
    id::mint()
}
