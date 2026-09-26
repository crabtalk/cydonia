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
    gpui::{Entity, FocusHandle},
    ui::input::TextField,
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
    /// The entry this pane is on, named the way a space names its members —
    /// which project it is in and which of that project's things it is. That
    /// is what ties a pane to a member of the arrangement.
    ///
    /// Nothing for the pane a window with no space open shows: it is on
    /// whatever the project was last left on, and the project holds that.
    pub(crate) entry: Option<artifact::space::Member>,
    /// Tracked on the pane this leaf draws, so the pane is an ancestor of the
    /// focused element and the chords claimed on it are reached — an action
    /// runs only through the focused element's ancestors. Where the focus lands
    /// for a pane holding nothing to type into; a session's composer and a
    /// document's editor are inside the pane already.
    pub(crate) focus: FocusHandle,
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
    pub(crate) open_card: Option<board::OpenCard>,
    pub(crate) card_drafts: Vec<board::CardDraft>,
    pub(crate) card_field: Entity<TextField>,
    /// The board's find field, and whether its bar is up.
    ///
    /// The bar stands whenever the query does: a board narrowed with nothing on
    /// screen saying so is a board quietly missing cards, and a drag that lands
    /// in a lane it cannot see is worse. Closing it is what clears the query —
    /// see [`crate::view::root::Cydonia::dismiss_find`].
    pub(crate) find_field: Entity<TextField>,
    pub(crate) finding: bool,
    /// Where the card now in the air would land. Written by the lanes and
    /// cards the pointer crosses and read by the one that draws the mark —
    /// see [`board::Landing`].
    pub(crate) landing: Option<board::Landing>,
    /// Where the list group now in the air would land — see
    /// [`board::GroupLanding`].
    pub(crate) group_landing: Option<board::GroupLanding>,
    /// What the table pane's field is attached to, and the field itself.
    pub(crate) cell: Option<table::Cell>,
    pub(crate) cell_field: Entity<TextField>,
    /// The formatting bar over the open document's selection, and the URL
    /// field it puts up — see [`crate::view::component::ribbon`].
    pub(crate) ribbon: Ribbon,
}

impl Leaf {
    pub(crate) fn new(
        focus: FocusHandle,
        composer: Entity<Composer>,
        card_field: Entity<TextField>,
        cell_field: Entity<TextField>,
        find_field: Entity<TextField>,
        ribbon: Ribbon,
    ) -> Self {
        Self {
            entry: None,
            focus,
            pane: Pane::Chat,
            composer,
            queued_galleries: Default::default(),
            asked_session: false,
            editing: None,
            open_card: None,
            card_drafts: Vec::new(),
            card_field,
            find_field,
            finding: false,
            landing: None,
            group_landing: None,
            cell: None,
            cell_field,
            ribbon,
        }
    }
}
