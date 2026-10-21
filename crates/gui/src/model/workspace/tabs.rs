//! The window's strip: its tabs, each on one entry or on a space.
//!
//! A continuation of [`Workspace`]'s one `impl`, the way [`super::spaces`] is.
//!
//! Saved in `state.toml`. A space's tab names the space by id, and closing it
//! deletes the space.
use super::*;
use artifact::{
    kind::Kind,
    space::{Member, Space},
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// How an entry opens in the window's strip — see [`Workspace::open_tab`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// In the preview tab's place, as the next preview.
    Preview,
    /// As a tab of its own.
    Kept,
}

/// One tab of the window's strip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Slot {
    /// One entry, or an app.
    Entry(Member),
    /// A space, by its id.
    Space { space: String },
}

impl Slot {
    /// The entry it is on. Nothing for a space.
    pub fn entry(&self) -> Option<&Member> {
        match self {
            Self::Entry(member) => Some(member),
            Self::Space { .. } => None,
        }
    }

    /// The space it is on, by id. Nothing for an entry.
    pub fn space(&self) -> Option<&str> {
        match self {
            Self::Space { space } => Some(space),
            Self::Entry(_) => None,
        }
    }
}

impl Workspace {
    /// The window's tabs, in strip order.
    pub fn strip(&self) -> &[Slot] {
        &self.strip
    }

    /// The tab in front: the one recorded, where the strip still holds it,
    /// or else its first.
    pub fn front_slot(&self) -> Option<&Slot> {
        self.front
            .as_ref()
            .filter(|front| self.strip.contains(front))
            .or_else(|| self.strip.first())
    }

    /// What the window lays out under its strip: the space in front, or the
    /// entry in front as a layout of one. Nothing where no tab is open.
    pub fn arranged(&self) -> Option<Cow<'_, Space>> {
        match self.front_slot()? {
            Slot::Space { .. } => self.active_space().map(Cow::Borrowed),
            Slot::Entry(member) => Some(Cow::Owned(Space::new(String::new(), "", member.clone()))),
        }
    }

    /// The entries the strip holds as tabs of their own, in strip order.
    pub fn tab_strip(&self) -> Vec<Member> {
        self.strip.iter().filter_map(Slot::entry).cloned().collect()
    }

    /// The tab the next opening replaces rather than adds to — see
    /// [`Self::open_tab`].
    pub fn preview_tab(&self) -> Option<&Member> {
        self.preview.as_ref()
    }

    /// Put `member` in front of the window's strip.
    ///
    /// A tab already in the strip is brought forward, and kept where it is
    /// opened [`Opening::Kept`]. Otherwise a preview takes the preview tab's
    /// place, and a kept tab, or a preview where there is none, lands after
    /// the tab in front.
    pub fn open_tab(&mut self, member: Member, opening: Opening, cx: &mut Context<Self>) {
        let keep = opening == Opening::Kept;
        let slot = Slot::Entry(member.clone());
        if !self.strip.contains(&slot) {
            let preview = match keep {
                true => None,
                false => self
                    .preview
                    .take()
                    .map(Slot::Entry)
                    .and_then(|preview| self.strip.iter().position(|tab| *tab == preview)),
            };
            match preview {
                Some(at) => self.strip[at] = slot.clone(),
                None => self.insert_slot(slot.clone()),
            }
            if !keep {
                self.preview = Some(member);
            }
        } else if keep && self.preview.as_ref() == Some(&member) {
            self.preview = None;
        }
        self.front = Some(slot);
        self.save();
        cx.notify();
    }

    /// Put the space with `id` in front of the strip, adding its tab after the
    /// tab in front where the strip does not hold it yet.
    pub(super) fn open_space_tab(&mut self, id: &str) {
        let slot = Slot::Space {
            space: id.to_owned(),
        };
        if !self.strip.contains(&slot) {
            self.insert_slot(slot.clone());
        }
        self.front = Some(slot);
    }

    /// Bring a tab the strip holds to its front.
    pub fn front_tab(&mut self, slot: &Slot, cx: &mut Context<Self>) {
        if self.strip.contains(slot) && self.front.as_ref() != Some(slot) {
            self.front = Some(slot.clone());
            self.save();
            cx.notify();
        }
    }

    /// Move the tab `moving` to where `to` sits in the strip.
    pub fn reorder_slot(&mut self, moving: &Slot, to: &Slot, cx: &mut Context<Self>) {
        let (Some(from), Some(at)) = (
            self.strip.iter().position(|tab| tab == moving),
            self.strip.iter().position(|tab| tab == to),
        ) else {
            return;
        };
        if from == at {
            return;
        }
        let moved = self.strip.remove(from);
        self.strip.insert(at, moved);
        self.save();
        cx.notify();
    }

    /// Keep the preview tab where it is `member`: the next opening adds a tab
    /// rather than replacing it.
    pub fn keep_tab(&mut self, member: &Member, cx: &mut Context<Self>) {
        if self.preview.as_ref() == Some(member) {
            self.preview = None;
            cx.notify();
        }
    }

    /// Keep the preview tab where it is on the entry of `kind` filed as `id`,
    /// which was just edited. Answers whether it was.
    pub(super) fn keep_edited(&mut self, kind: Kind, id: &str) -> bool {
        let edited = self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.kind() == kind && preview.id() == id);
        if edited {
            self.preview = None;
        }
        edited
    }

    /// Take a tab off the strip and nothing else. The one in front hands the
    /// front to its neighbour, to its left or, for the first, to its right.
    pub(super) fn take_slot(&mut self, slot: &Slot, cx: &mut Context<Self>) {
        let Some(at) = self.strip.iter().position(|tab| tab == slot) else {
            return;
        };
        if slot.entry() == self.preview.as_ref() {
            self.preview = None;
        }
        let front = self.front_slot().cloned();
        self.strip.remove(at);
        if front.as_ref() == Some(slot) {
            self.front = self
                .strip
                .get(at.saturating_sub(1))
                .or_else(|| self.strip.first())
                .cloned();
        }
        self.save();
        cx.notify();
    }

    /// Take the entry `member`'s tab off the strip.
    pub fn close_tab(&mut self, member: &Member, cx: &mut Context<Self>) {
        self.take_slot(&Slot::Entry(member.clone()), cx);
    }

    /// Put `slot` in `taken`'s place in the strip, in front where `taken` was.
    pub(super) fn replace_slot(&mut self, taken: &Slot, slot: Slot) {
        let front = self.front_slot() == Some(taken);
        match self.strip.iter().position(|tab| tab == taken) {
            Some(at) => self.strip[at] = slot.clone(),
            None => self.insert_slot(slot.clone()),
        }
        self.strip.dedup();
        if front {
            self.front = Some(slot);
        }
    }

    /// Take every entry `gone` names off the strip, as [`Self::close_tab`]
    /// does one.
    pub(super) fn drop_tabs(&mut self, gone: &[Member], cx: &mut Context<Self>) {
        for member in gone {
            self.close_tab(member, cx);
        }
    }

    /// A tab after the one in front, or at the end where none is.
    fn insert_slot(&mut self, slot: Slot) {
        let at = self
            .front_slot()
            .and_then(|front| self.strip.iter().position(|tab| tab == front))
            .map_or(self.strip.len(), |at| at + 1);
        self.strip.insert(at, slot);
    }

    /// The tabs `state.toml` holds, or else the entry the active project last
    /// showed — and the space the window was on, where it holds no tab for
    /// it, then every space still without one. Tabs on what has since gone
    /// are left out.
    pub(super) fn restore_tabs(
        &mut self,
        tabs: Option<Vec<Slot>>,
        front: Option<Slot>,
        space: Option<String>,
        cx: &App,
    ) {
        let mut strip = tabs.unwrap_or_else(|| {
            self.landed()
                .map(|(project, entry)| {
                    Slot::Entry(Member::new(project, entry.kind, entry.id.clone()))
                })
                .into_iter()
                .collect()
        });
        let mut front = front;
        if let Some(space) = space {
            let slot = Slot::Space { space };
            if !strip.contains(&slot) {
                strip.push(slot.clone());
                front = Some(slot);
            }
        }
        self.strip = strip
            .into_iter()
            .filter(|slot| match slot {
                Slot::Space { space } => self.space_ix(space).is_some(),
                Slot::Entry(member) => self.lists(member, cx),
            })
            .collect();
        // Every space keeps a tab on the strip.
        for space in &self.spaces {
            let slot = Slot::Space {
                space: space.id.clone(),
            };
            if !self.strip.contains(&slot) {
                self.strip.push(slot);
            }
        }
        self.front = front;
        self.save();
    }

    /// Bring the window's front tab up and put the project's selection on it.
    pub(super) fn bring_up_tabs(&mut self, cx: &mut Context<Self>) {
        match self.front_slot().cloned() {
            Some(Slot::Entry(member)) => {
                if let Some((project, showing)) = self.showing_of(&member, cx) {
                    self.select_showing(project, showing, cx);
                }
            }
            Some(Slot::Space { .. }) => self.bring_up_space(cx),
            None => {}
        }
    }
}
