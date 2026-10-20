//! The window's strip of tabs: what it shows while no space is open.
//!
//! A continuation of [`Workspace`]'s one `impl`, the way [`super::spaces`] is.
//!
//! Held as a [`Space`] of one pane that is never written to the shelf, and
//! saved in `state.toml` instead — see [`Workspace::arranged`].
use super::*;
use artifact::{
    kind::Kind,
    space::{Member, Node, Space},
};

/// How an entry opens in the window's strip — see [`Workspace::open_tab`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// In the preview tab's place, as the next preview.
    Preview,
    /// As a tab of its own.
    Kept,
}

impl Workspace {
    /// What the window is arranged by: the open space, or else its own tabs.
    /// Nothing where no space is open and no tab is either.
    pub fn arranged(&self) -> Option<&Space> {
        self.active_space().or(self.tabs.as_ref())
    }

    /// The window's tabs, in strip order. Empty where none is open.
    pub fn tab_strip(&self) -> Vec<Member> {
        self.tabs
            .as_ref()
            .map(|tabs| tabs.tree.stack())
            .unwrap_or_default()
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
        let mut strip = self.tab_strip();
        let front = self.tabs.as_ref().and_then(|tabs| tabs.tree.front());
        if !strip.contains(&member) {
            let preview = match keep {
                true => None,
                false => self
                    .preview
                    .take()
                    .filter(|preview| strip.contains(preview)),
            };
            match preview {
                Some(preview) => {
                    let at = strip.iter().position(|tab| *tab == preview).unwrap_or(0);
                    strip[at] = member.clone();
                }
                None => {
                    let at = front
                        .and_then(|front| strip.iter().position(|tab| *tab == front))
                        .map_or(strip.len(), |at| at + 1);
                    strip.insert(at, member.clone());
                }
            }
            if !keep {
                self.preview = Some(member.clone());
            }
        } else if keep && self.preview.as_ref() == Some(&member) {
            self.preview = None;
        }
        self.set_strip(strip, Some(member));
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

    /// Take `member` out of the window's strip. The last one out leaves the
    /// window on no tab.
    pub fn close_tab(&mut self, member: &Member, cx: &mut Context<Self>) {
        let strip = self.tab_strip();
        if !strip.contains(member) {
            return;
        }
        if self.preview.as_ref() == Some(member) {
            self.preview = None;
        }
        let front = self
            .tabs
            .as_ref()
            .and_then(|tabs| tabs.tree.front())
            .filter(|front| front != member);
        let strip = strip.into_iter().filter(|tab| tab != member).collect();
        self.set_strip(strip, front);
        self.save();
        cx.notify();
    }

    /// Take every member `gone` names out of the window's strip, as
    /// [`Self::close_tab`] does one.
    pub(super) fn drop_tabs(&mut self, gone: &[Member], cx: &mut Context<Self>) {
        let strip = self.tab_strip();
        if !strip.iter().any(|tab| gone.contains(tab)) {
            return;
        }
        for member in gone {
            self.close_tab(member, cx);
        }
    }

    /// The window's strip as `strip`, with `front` in front where it holds it.
    pub(super) fn set_strip(&mut self, strip: Vec<Member>, front: Option<Member>) {
        let mut strip = strip.into_iter();
        let Some(first) = strip.next() else {
            self.tabs = None;
            self.preview = None;
            return;
        };
        let tree = Node::Leaf {
            ratio: 1.,
            entry: first,
            tabs: strip.collect(),
            front,
        };
        match &mut self.tabs {
            Some(tabs) => tabs.tree = tree,
            None => {
                self.tabs = Some(Space {
                    id: String::new(),
                    touched: 0,
                    name: String::new(),
                    zoomed: None,
                    focused: None,
                    tree,
                })
            }
        }
    }

    /// The tabs `state.toml` holds, or else the entry the active project last
    /// showed.
    pub(super) fn restore_tabs(
        &mut self,
        tabs: Option<Vec<Member>>,
        front: Option<Member>,
        cx: &App,
    ) {
        let tabs = tabs.unwrap_or_else(|| {
            self.landed()
                .map(|(project, entry)| Member::new(project, entry.kind, entry.id.clone()))
                .into_iter()
                .collect()
        });
        let tabs: Vec<Member> = tabs
            .into_iter()
            .filter(|member| match member {
                Member::App { app } => self.apps.iter().any(|held| held.id == *app),
                Member::Entry(_) => self.showing_of(member, cx).is_some(),
            })
            .collect();
        self.set_strip(tabs, front);
        self.save();
    }

    /// Bring the window's front tab up and put the project's selection on it.
    pub(super) fn bring_up_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(front) = self.tabs.as_ref().and_then(|tabs| tabs.tree.front()) else {
            return;
        };
        if let Some((project, showing)) = self.showing_of(&front, cx) {
            self.select_showing(project, showing, cx);
        }
    }
}
