//! A project's spaces: several entries on screen at once, and how they are
//! arranged.
//!
//! One file per space under `.cydonia/spaces/`, named for the millisecond it
//! was made, the way boards are.
//!
//! A space holds no entry. It holds the ids of entries that exist beside it,
//! each by its project's path — see [`Entry`] — so the same session can be a
//! member of two spaces and stay one session. A member whose entry has been
//! deleted resolves to nothing.
//!
//! The tree nests: children are a `Vec`, so a third pane alongside two others
//! joins their split rather than nesting inside one of them.

use crate::{kind::Kind, stamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// What a space is shown as before it is named.
pub const UNNAMED: &str = "Untitled";

/// The stem every auto-given name is built on: `space-1`, `space-2`.
const STEM: &str = "space";

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
    fn axis(self) -> Axis {
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
    fn opposite(self) -> Self {
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
/// Generic over what a pane is *on*, because none of the work here reads a
/// member: splitting, walking, swapping and sizing only ever ask whether two
/// are the same one.
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
pub enum Node<T> {
    /// The entry a pane is on. What it may since have become — deleted,
    /// renamed, in a project no longer open — is the caller's to resolve, and
    /// a pane whose entry answers nothing draws as an empty one.
    Leaf {
        #[serde(default = "whole")]
        ratio: f64,
        /// The first of the entries this pane holds. Not "the one in front":
        /// that is `front`. An entry is in one pane at a time, so any of them
        /// names the pane.
        entry: T,
        /// The rest of them, in the order the stack draws them behind `entry`.
        /// Empty for the pane that holds one thing, which is most of them.
        #[serde(default = "none", skip_serializing_if = "Vec::is_empty")]
        tabs: Vec<T>,
        /// The tab in front. Read through [`Node::front`]: one no longer in
        /// the stack, or none written, is the first.
        #[serde(default = "first", skip_serializing_if = "Option::is_none")]
        front: Option<T>,
    },
    Split {
        #[serde(default = "whole")]
        ratio: f64,
        axis: Axis,
        children: Vec<Node<T>>,
    },
}

fn whole() -> f64 {
    1.
}

/// A pane holding nothing but its name — the shape every leaf written before
/// tabs existed reads back as.
fn none<T>() -> Vec<T> {
    Vec::new()
}

/// A pane on its first tab — every leaf written before `front` existed.
fn first<T>() -> Option<T> {
    None
}

/// A leaf taking `ratio` of whatever it is put in.
fn sized<T>(entry: T, ratio: f64) -> Node<T> {
    Node::Leaf {
        ratio,
        entry,
        tabs: Vec::new(),
        front: None,
    }
}

impl<T: Clone + PartialEq> Node<T> {
    pub fn leaf(entry: T) -> Self {
        Self::Leaf {
            ratio: whole(),
            entry,
            tabs: Vec::new(),
            front: None,
        }
    }

    /// The entries one pane holds, in the order its stack lists them. Empty
    /// for a split, which holds none of its own.
    pub fn stack(&self) -> Vec<T> {
        match self {
            Self::Leaf { entry, tabs, .. } => std::iter::once(entry).chain(tabs).cloned().collect(),
            Self::Split { .. } => Vec::new(),
        }
    }

    /// The tab this pane shows. `None` for a split.
    pub fn front(&self) -> Option<T> {
        let Self::Leaf {
            entry, tabs, front, ..
        } = self
        else {
            return None;
        };
        Some(
            front
                .as_ref()
                .filter(|front| *front == entry || tabs.contains(front))
                .unwrap_or(entry)
                .clone(),
        )
    }

    /// Bring `tab` to the front of the pane holding it. Answers whether that
    /// changed what the pane shows.
    pub fn set_front(&mut self, tab: &T) -> bool {
        if let Self::Split { children, .. } = self {
            return children.iter_mut().any(|child| child.set_front(tab));
        }
        if !self.holds(tab) || self.front().as_ref() == Some(tab) {
            return false;
        }
        if let Self::Leaf { front, .. } = self {
            *front = Some(tab.clone());
        }
        true
    }

    /// The tab each pane shows, in the order the panes are laid out.
    pub fn fronts(&self) -> Vec<T> {
        match self {
            Self::Leaf { .. } => self.front().into_iter().collect(),
            Self::Split { children, .. } => children.iter().flat_map(Node::fronts).collect(),
        }
    }

    /// The tab each pane along the top edge shows, left to right.
    pub fn top_fronts(&self) -> Vec<T> {
        match self {
            Self::Leaf { .. } => self.front().into_iter().collect(),
            Self::Split {
                axis: Axis::Horizontal,
                children,
                ..
            } => children.iter().flat_map(Node::top_fronts).collect(),
            Self::Split {
                axis: Axis::Vertical,
                children,
                ..
            } => children.first().map(Node::top_fronts).unwrap_or_default(),
        }
    }

    /// Whether this node is the pane holding `entry` — as its name or as one
    /// of its tabs. A split holds nothing itself.
    fn holds(&self, entry: &T) -> bool {
        match self {
            Self::Leaf {
                entry: named, tabs, ..
            } => named == entry || tabs.contains(entry),
            Self::Split { .. } => false,
        }
    }

    pub fn split(axis: Axis, children: Vec<Node<T>>) -> Self {
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
    /// one launch and the next, and a space written in pixels would come back
    /// either overflowing it or leaving a strip of it empty.
    pub fn ratio(&self) -> f64 {
        match self {
            Self::Leaf { ratio, .. } | Self::Split { ratio, .. } => *ratio,
        }
    }

    fn set_ratio(&mut self, to: f64) {
        match self {
            Self::Leaf { ratio, .. } | Self::Split { ratio, .. } => *ratio = to,
        }
    }

    /// Every entry number below here, in the order the panes are laid out.
    pub fn entries(&self) -> Vec<T> {
        match self {
            Self::Leaf { .. } => self.stack(),
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

    /// Take every one of `gone` out of whatever pane holds it, dropping the
    /// panes and splits left with nothing. Answers whether anything went.
    ///
    /// A pane whose name goes while it still holds tabs keeps the pane: the
    /// first tab takes the name, and the stack loses an entry rather than the
    /// window losing a pane.
    ///
    /// A split down to one child is replaced by that child: a division with
    /// nothing on one side of it is a divider the pointer can still catch.
    ///
    /// What the departed held is shared out among what is left, so a split
    /// still covers the room it was given.
    pub fn prune(&mut self, gone: &[T]) -> bool {
        if let Self::Leaf {
            entry, tabs, front, ..
        } = self
        {
            if front.as_ref().is_some_and(|shown| gone.contains(shown)) {
                *front = None;
            }
            let before = tabs.len();
            tabs.retain(|tab| !gone.contains(tab));
            let mut changed = tabs.len() != before;
            if gone.contains(entry) && !tabs.is_empty() {
                *entry = tabs.remove(0);
                changed = true;
            }
            return changed;
        }
        let Self::Split { children, .. } = self else {
            return false;
        };
        let before = children.len();
        let mut changed = false;
        for child in children.iter_mut() {
            changed |= child.prune(gone);
        }
        // The recursion above has already promoted a tab into any pane whose
        // name went, so a leaf still named by one of `gone` is a pane with
        // nothing left in it.
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
    pub fn contains(&self, entry: &T) -> bool {
        match self {
            Self::Leaf { .. } => self.holds(entry),
            Self::Split { children, .. } => children.iter().any(|child| child.contains(entry)),
        }
    }

    /// Put `arriving` at the end of the stack of the pane holding `target` —
    /// a drop on a pane's bar rather than on its edge. Answers whether that
    /// pane was found.
    ///
    /// At the end, never in front of what is there: the stack's order is the
    /// order tabs arrived in, and one that reshuffled itself would move the
    /// tab under the pointer out from under it.
    pub fn stack_onto(&mut self, target: &T, arriving: &T) -> bool {
        if self.holds(target) {
            let held = self.holds(arriving);
            if let Self::Leaf { tabs, .. } = self
                && !held
            {
                tabs.push(arriving.clone());
            }
            return true;
        }
        match self {
            Self::Leaf { .. } => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.stack_onto(target, arriving)),
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
    pub fn insert(&mut self, target: &T, arriving: &T, side: Side) -> bool {
        if self.holds(target)
            && let Self::Leaf {
                entry,
                ratio,
                tabs,
                front,
            } = self
        {
            let ratio = *ratio;
            // The pane divides whole: its tabs go with it rather than being
            // scattered across the new seam.
            let kept = Self::Leaf {
                ratio: 0.5,
                entry: entry.clone(),
                tabs: std::mem::take(tabs),
                front: front.take(),
            };
            // The arrival takes the side it was dropped on, so the pane that
            // was already there does not jump across the new seam.
            let children = match side.after() {
                true => vec![kept, sized(arriving.clone(), 0.5)],
                false => vec![sized(arriving.clone(), 0.5), kept],
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
        let at = children.iter().position(|child| child.holds(target));
        if let Some(at) = at {
            if axis == side.axis() {
                // Halve what the dropped-on pane holds and hand the arrival
                // the other half: every other child keeps its share.
                let share = children[at].ratio() / 2.;
                children[at].set_ratio(share);
                children.insert(
                    at + usize::from(side.after()),
                    sized(arriving.clone(), share),
                );
                return true;
            }
            // Across the split's own direction, so the pane divides in place.
            return children[at].insert(target, arriving, side);
        }
        children
            .iter_mut()
            .any(|child| child.insert(target, arriving, side))
    }

    /// Take the pane showing this entry out, collapsing whatever it leaves
    /// behind. Answers whether it was there.
    pub fn remove(&mut self, entry: &T) -> bool {
        if !self.contains(entry) {
            return false;
        }
        self.prune(std::slice::from_ref(entry))
    }

    /// The node at this path: each step is a child's index in the split
    /// above it, so an empty path is this node.
    pub fn at_path_mut(&mut self, path: &[usize]) -> Option<&mut Node<T>> {
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
    pub fn path_to(&self, entry: &T) -> Option<Vec<usize>> {
        match self {
            Self::Leaf { .. } => self.holds(entry).then(Vec::new),
            Self::Split { children, .. } => children.iter().enumerate().find_map(|(ix, child)| {
                let mut path = child.path_to(entry)?;
                path.insert(0, ix);
                Some(path)
            }),
        }
    }

    /// The node at this path — see [`Self::at_path_mut`].
    pub fn at_path(&self, path: &[usize]) -> Option<&Node<T>> {
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
    fn edge_leaf(&self, side: Side) -> Option<T> {
        match self {
            Self::Leaf { entry, .. } => Some(entry.clone()),
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
    pub fn neighbour(&self, entry: &T, side: Side) -> Option<T> {
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
    ///
    /// Whole panes, tabs and all: `a` and `b` name the panes holding them, and
    /// a pane of three tabs crossing a seam arrives with the three.
    pub fn swap(&mut self, a: &T, b: &T) -> bool {
        let (Some(here), Some(there)) = (self.path_to(a), self.path_to(b)) else {
            return false;
        };
        if here == there {
            return false;
        }
        let (Some(from), Some(to)) = (
            self.at_path(&here).map(Node::held),
            self.at_path(&there).map(Node::held),
        ) else {
            return false;
        };
        self.put_stack(&here, to);
        self.put_stack(&there, from);
        true
    }

    /// A pane's stack and the front it holds, as [`Self::put_stack`] takes
    /// them.
    fn held(&self) -> (Vec<T>, Option<T>) {
        match self {
            Self::Leaf { front, .. } => (self.stack(), front.clone()),
            Self::Split { .. } => (Vec::new(), None),
        }
    }

    /// Stand the pane at this path on `stack` with `shown` in front, keeping
    /// the room it was given. The stack is never empty — it came off a pane,
    /// and every pane has a name.
    fn put_stack(&mut self, path: &[usize], (mut stack, shown): (Vec<T>, Option<T>)) {
        let Some(Self::Leaf {
            entry, tabs, front, ..
        }) = self.at_path_mut(path)
        else {
            return;
        };
        if stack.is_empty() {
            return;
        }
        *entry = stack.remove(0);
        *tabs = stack;
        *front = shown;
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

    /// Whether the pane on `entry` can move to `target`'s edge: both are
    /// here, and they are different entries.
    pub fn relocatable(&self, entry: &T, target: &T) -> bool {
        entry != target && self.contains(entry) && self.contains(target)
    }

    /// Move a pane already here to another pane's edge.
    ///
    /// Taken out before it is put back, so the split it leaves collapses the
    /// way it would have if the pane had been closed — a pane dragged out of a
    /// pair does not leave the seam it was holding.
    pub fn relocate(&mut self, entry: &T, target: &T, side: Side) -> bool {
        if !self.relocatable(entry, target) {
            return false;
        }
        self.remove(entry);
        self.insert(target, entry, side)
    }
}

/// What a pane is on: one entry of a project, or an app.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Member {
    Entry(Entry),
    /// An app, by its id. It belongs to no project.
    App {
        app: String,
    },
}

/// One entry of a project: which project it is in, and which of that
/// project's things it is.
///
/// The project by its path, because a space spans them — it is kept beside
/// the app's own config rather than inside any one project, and absolute paths
/// are what it can name from there. That is also why a space does not travel
/// with a repository: these paths are this machine's.
///
/// By `id` rather than by the `#number` [`crate::entry`] gives out: numbers are
/// per project, so two projects' `#12` are different entries, and a member has
/// to say which it means without reading either project's database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "Written")]
pub struct Entry {
    pub project: std::path::PathBuf,
    pub kind: Kind,
    pub id: String,
}

/// An [`Entry`] as a file holds it, before its id is read — see
/// [`crate::article::stored_id`].
#[derive(Deserialize)]
struct Written {
    project: std::path::PathBuf,
    kind: Kind,
    id: String,
}

impl From<Written> for Entry {
    fn from(written: Written) -> Self {
        let id = crate::article::stored_id(written.kind, written.id);
        Self {
            project: written.project,
            kind: written.kind,
            id,
        }
    }
}

impl Member {
    /// An entry of the project at `project`.
    pub fn new(project: impl Into<std::path::PathBuf>, kind: Kind, id: impl Into<String>) -> Self {
        Self::Entry(Entry {
            project: project.into(),
            kind,
            id: id.into(),
        })
    }

    /// The app `id`.
    pub fn app(id: impl Into<String>) -> Self {
        Self::App { app: id.into() }
    }

    /// The entry it names. `None` for an app.
    pub fn entry(&self) -> Option<&Entry> {
        match self {
            Self::Entry(entry) => Some(entry),
            Self::App { .. } => None,
        }
    }

    /// The same, owned.
    pub fn into_entry(self) -> Option<Entry> {
        match self {
            Self::Entry(entry) => Some(entry),
            Self::App { .. } => None,
        }
    }

    /// The project an entry is in. `None` for an app.
    pub fn project(&self) -> Option<&std::path::Path> {
        self.entry().map(|entry| entry.project.as_path())
    }

    pub fn kind(&self) -> Kind {
        match self {
            Self::Entry(entry) => entry.kind,
            Self::App { .. } => Kind::App,
        }
    }

    /// An entry's id in its project, or an app's id.
    pub fn id(&self) -> &str {
        match self {
            Self::Entry(entry) => &entry.id,
            Self::App { app } => app,
        }
    }
}

impl From<Entry> for Member {
    fn from(entry: Entry) -> Self {
        Self::Entry(entry)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    /// What names this space for as long as it exists, and what its file is
    /// called. Written into the file as well, the way a board's is: a backend
    /// that keeps spaces in a row has no filename to fall back on.
    #[serde(default)]
    pub id: String,
    /// When it was last written, as the backend counts. Never written into the
    /// file, which would be a second copy able to disagree.
    #[serde(skip)]
    pub touched: u128,
    /// Given by [`next_name`] when the space is made, and replaced by
    /// whatever it is renamed to.
    #[serde(default)]
    pub name: String,
    /// An entry of the pane standing over the others, if one is. The tree
    /// keeps its shape while it does: unzooming puts every pane back where it
    /// was, at the size it was. Read through [`Space::zoomed`].
    ///
    /// Before `tree` in the struct, and so in the file: `tree` is a table, and
    /// TOML reads a bare key after one as belonging to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed: Option<Member>,
    /// The tab last focused. Read through [`Space::focused`]: the pane is
    /// whichever holds it now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<Member>,
    pub tree: Node<Member>,
}

impl Space {
    /// A space over one entry. Spaces are made by dragging a second entry
    /// onto the first, so the one already open is what a new one starts from.
    pub fn new(id: String, name: &str, entry: Member) -> Self {
        Self {
            id,
            touched: stamp::now(),
            name: name.to_owned(),
            zoomed: None,
            focused: None,
            tree: Node::leaf(entry),
        }
    }

    /// The name its tab shows.
    pub fn label(&self) -> &str {
        match self.name.is_empty() {
            true => UNNAMED,
            false => &self.name,
        }
    }

    pub fn entries(&self) -> Vec<Member> {
        self.tree.entries()
    }

    pub fn leaves(&self) -> usize {
        self.tree.leaves()
    }

    /// Drop members that no longer resolve — see [`Node::prune`].
    pub fn prune(&mut self, gone: &[Member]) -> bool {
        self.leaving(gone);
        self.tree.prune(gone)
    }

    /// Hand `zoomed` and `focused`, where `gone` holds the entry either names,
    /// to a tab of the same pane that stays. Left as they are where none
    /// stays.
    fn leaving(&mut self, gone: &[Member]) {
        let handed = |held: &Option<Member>| {
            let held = held.as_ref().filter(|held| gone.contains(held))?;
            self.stack_of(held)
                .into_iter()
                .find(|tab| !gone.contains(tab))
        };
        let (zoomed, focused) = (handed(&self.zoomed), handed(&self.focused));
        if zoomed.is_some() {
            self.zoomed = zoomed;
        }
        if focused.is_some() {
            self.focused = focused;
        }
    }

    pub fn contains(&self, entry: &Member) -> bool {
        self.tree.contains(entry)
    }

    /// Put an entry beside one already here — a drag from the sidebar onto a
    /// pane's edge. Answers whether the pane dropped on was found.
    ///
    /// An entry already shown is moved rather than shown twice.
    pub fn insert(&mut self, target: &Member, arriving: &Member, side: Side) -> bool {
        match self.contains(arriving) {
            true => self.relocate(arriving, target, side),
            false => self.tree.insert(target, arriving, side),
        }
    }

    /// Move a pane already here to another pane's edge — see
    /// [`Node::relocate`].
    pub fn relocate(&mut self, entry: &Member, target: &Member, side: Side) -> bool {
        if !self.tree.relocatable(entry, target) {
            return false;
        }
        self.leaving(std::slice::from_ref(entry));
        self.tree.relocate(entry, target, side)
    }

    /// Put an entry into the pane holding `target`, at the end of its stack —
    /// a drop on a pane's middle rather than on its edge. Answers whether
    /// that pane was found.
    ///
    /// An entry is in one pane at a time, so one already here is taken out of
    /// the pane it was in first.
    pub fn stack(&mut self, target: &Member, arriving: &Member) -> bool {
        // Already in that stack: a tab let go over its own bar is a drag that
        // changed nothing, and taking it out to put it back would send it to
        // the end of a stack the reader never asked to reorder.
        if self.stack_of(target).contains(arriving) {
            return false;
        }
        if self.contains(arriving) {
            // Taking it out can collapse the split it was holding, which is
            // why the target is found again afterwards rather than before.
            self.remove(arriving);
            if !self.contains(target) {
                return false;
            }
        }
        self.tree.stack_onto(target, arriving)
    }

    /// The entries the pane holding this one draws, in stack order. Empty for
    /// an entry no pane here is on.
    pub fn stack_of(&self, entry: &Member) -> Vec<Member> {
        self.pane_holding(entry)
            .map(Node::stack)
            .unwrap_or_default()
    }

    /// The tab the pane holding this entry shows — see [`Node::front`].
    pub fn front_of(&self, entry: &Member) -> Option<Member> {
        self.pane_holding(entry)?.front()
    }

    /// The pane holding this entry.
    fn pane_holding(&self, entry: &Member) -> Option<&Node<Member>> {
        self.tree.at_path(&self.tree.path_to(entry)?)
    }

    /// Bring a tab to the front of its pane — see [`Node::set_front`].
    pub fn set_front(&mut self, tab: &Member) -> bool {
        self.tree.set_front(tab)
    }

    /// Take one entry out. A pane holding tabs keeps the pane and loses a tab;
    /// the last one out closes the pane. The space stays when its last pane
    /// goes; the workspace deletes a space instead of taking one of its last
    /// two entries out.
    ///
    /// A zoomed pane that is closed leaves the rest unzoomed rather than
    /// standing something else in its place.
    pub fn remove(&mut self, entry: &Member) -> bool {
        self.leaving(std::slice::from_ref(entry));
        self.tree.remove(entry)
    }

    /// Stand one pane over the others, or put it back. Answers what is zoomed
    /// afterwards.
    ///
    /// Zooming the pane already zoomed unzooms it, and zooming another swaps
    /// to it — one pane is over the rest, or none is.
    ///
    /// Held by whichever tab the caller named, so a pane zoomed from one of
    /// its tabs stays zoomed when another tab comes forward.
    pub fn zoom(&mut self, entry: &Member) -> Option<Member> {
        let again = self
            .zoomed()
            .is_some_and(|zoomed| self.stack_of(entry).contains(&zoomed));
        self.zoomed = (!again && self.contains(entry)).then(|| entry.clone());
        self.zoomed()
    }

    /// Record `tab` as the one focused. Answers whether that changed what is
    /// recorded.
    pub fn focus(&mut self, tab: &Member) -> bool {
        if !self.contains(tab) || self.focused.as_ref() == Some(tab) {
            return false;
        }
        self.focused = Some(tab.clone());
        true
    }

    /// The entry in front of the pane holding the one last focused, if it is
    /// still here.
    pub fn focused(&self) -> Option<Member> {
        self.front_of(self.focused.as_ref()?)
    }

    /// The entry in front of the pane last focused, or else of the first pane
    /// laid out.
    pub fn focused_front(&self) -> Option<Member> {
        self.focused()
            .or_else(|| self.tree.fronts().first().cloned())
    }

    /// The entry in front of the pane standing over the others, if it is still
    /// here.
    pub fn zoomed(&self) -> Option<Member> {
        self.front_of(self.zoomed.as_ref()?)
    }
}

/// The name a new space takes: `space-1`, then `space-2`.
///
/// Counted above the highest taken rather than into the first gap. A name is
/// how a space is asked for, and one reused after a delete would answer for a
/// space the asker never saw.
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
