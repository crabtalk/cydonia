//! One pane's own state: what it is showing, and everything the showing of it
//! needs.
//!
//! Split out of [`crate::view::root::Cydonia`], which held all of this when a
//! window showed one entry at a time. A second pane is a second composer, a
//! second board scroll and a second card in the air — one of each on the root
//! is what makes two panes impossible, not the drawing.
//!
//! Nothing here is the window's. The sidebar, the menus, the dialogs and the
//! settings window are one to a window however many panes it holds, and they
//! stay on the root.

use crate::{
    model::state,
    view::{
        board::{self, Editing},
        component::{composer::Composer, ribbon::Ribbon},
        table,
    },
};
use bezel::{
    gpui::{Entity, ScrollHandle},
    ui::{input::TextField, scroll::DriftState},
};

/// Which pane the detail column shows. A property of the pane, not of a
/// project — switching projects must not teleport you to another pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Board,
    Article,
    Table,
}

impl Pane {
    /// The pane a remembered entry is read in — see
    /// [`crate::model::workspace::Workspace::landing`].
    ///
    pub(crate) fn of(kind: state::Kind) -> Self {
        match kind {
            state::Kind::Session => Self::Chat,
            state::Kind::Board => Self::Board,
            state::Kind::Article => Self::Article,
            state::Kind::Table => Self::Table,
        }
    }
}

/// What one pane shows and holds while it shows it.
pub struct Leaf {
    /// The entry this pane is on, named the way a layout names its members —
    /// which project it is in and which of that project's things it is. That
    /// is what ties a pane to a member of the arrangement.
    ///
    /// Nothing for the pane a window with no layout open shows: it is on
    /// whatever the project was last left on, and the project holds that.
    pub(crate) entry: Option<artifact::layout::Member>,
    pub(crate) pane: Pane,
    pub(crate) composer: Entity<Composer>,
    pub(crate) queued_galleries: std::collections::HashMap<
        (u64, usize, String),
        Entity<crate::view::component::transcript::gallery::Gallery>,
    >,
    /// Whether a session has been asked for with no agent to open one on.
    ///
    /// The chat pane then stands over the notice — see
    /// [`crate::view::root::Cydonia::no_agent`] — rather than a ⌘N throwing the
    /// settings window up in front of a window that has said nothing about
    /// why. Runtime only, and read through
    /// [`crate::view::root::Cydonia::has_pane`], which drops it the moment an
    /// agent is there: a flag left set behind an install would keep an empty
    /// pane on offer.
    pub(crate) asked_session: bool,
    pub(crate) editing: Option<Editing>,
    pub(crate) card_field: Entity<TextField>,
    /// The board's own scroll, and the drift that carries a held card past the
    /// edge of the window — a lane out of sight is one a drag cannot reach,
    /// because reaching for it means letting go.
    pub(crate) board_scroll: ScrollHandle,
    pub(crate) board_drift: DriftState,
    /// The same, per lane — see [`board::Lanes`].
    pub(crate) lanes: board::Lanes,
    /// Where the card now in the air would land. Written by the lanes and
    /// cards the pointer crosses and read by the one that draws the mark —
    /// see [`board::Landing`].
    pub(crate) landing: Option<board::Landing>,
    /// What the table pane's field is attached to, and the field itself.
    pub(crate) cell: Option<table::Cell>,
    pub(crate) cell_field: Entity<TextField>,
    /// The formatting bar over the open document's selection, and the URL
    /// field it puts up — see [`crate::view::component::ribbon`].
    pub(crate) ribbon: Ribbon,
}

impl Leaf {
    pub(crate) fn new(
        composer: Entity<Composer>,
        card_field: Entity<TextField>,
        cell_field: Entity<TextField>,
        ribbon: Ribbon,
    ) -> Self {
        Self {
            entry: None,
            pane: Pane::Chat,
            composer,
            queued_galleries: Default::default(),
            asked_session: false,
            editing: None,
            card_field,
            board_scroll: ScrollHandle::new(),
            board_drift: DriftState::new(),
            lanes: board::Lanes::default(),
            landing: None,
            cell: None,
            cell_field,
            ribbon,
        }
    }
}
