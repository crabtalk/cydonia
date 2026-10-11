//! One pane's own state: what it is showing, and everything the showing of it
//! needs.
//!
//! Split out of [`crate::view::window::root::Cydonia`], which held all of this when a
//! window showed one entry at a time. A second pane is a second composer and a
//! second board scroll — one of each on the root is what makes two panes
//! impossible, not the drawing.
//!
//! Nothing here is the window's. The sidebar, the menus, the dialogs and the
//! settings window are one to a window however many panes it holds, and they
//! stay on the root.

use crate::view::{
    board::{self, Editing},
    component::{composer::Composer, ribbon::Ribbon},
    pane::drawer::Drawer,
    table,
};
use artifact::kind::Kind;
use bezel::{
    gpui::{Entity, FocusHandle},
    ui::input::TextField,
};

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
    pub(crate) pane: Kind,
    pub(crate) composer: Entity<Composer>,
    pub(crate) queued_galleries: std::collections::HashMap<
        (u64, usize, String),
        Entity<crate::view::component::transcript::gallery::Gallery>,
    >,
    /// Whether a session has been asked for with no agent to open one on.
    ///
    /// The chat pane then stands over the notice — see
    /// [`crate::view::window::root::Cydonia::no_agent`] — rather than a ⌘N throwing the
    /// settings window up in front of a window that has said nothing about
    /// why. Runtime only, and read through
    /// [`crate::view::window::root::Cydonia::has_pane`], which drops it the moment an
    /// agent is there: a flag left set behind an install would keep an empty
    /// pane on offer.
    pub(crate) asked_session: bool,
    pub(crate) editing: Option<Editing>,
    /// What the pane's drawer holds, while it is up — see
    /// [`crate::view::pane::drawer`].
    pub(crate) drawer: Option<Drawer>,
    pub(crate) card_drafts: Vec<board::CardDraft>,
    pub(crate) card_field: Entity<TextField>,
    /// The board's find field, and whether its bar is up.
    ///
    /// The bar stands whenever the query does: a board narrowed with nothing on
    /// screen saying so is a board quietly missing cards, and a drag that lands
    /// in a lane it cannot see is worse. Closing it is what clears the query —
    /// see [`crate::view::window::root::Cydonia::dismiss_find`].
    pub(crate) find_field: Entity<TextField>,
    pub(crate) finding: bool,
    /// Which find hit an article or a transcript is on, counted from the
    /// first — see [`crate::view::pane::find`].
    pub(crate) find_at: usize,
    /// Whether an article's or a transcript's find reads its field as a
    /// regular expression.
    pub(crate) find_pattern: bool,
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
            pane: Kind::Session,
            composer,
            queued_galleries: Default::default(),
            asked_session: false,
            editing: None,
            drawer: None,
            card_drafts: Vec::new(),
            card_field,
            find_field,
            finding: false,
            find_at: 0,
            find_pattern: false,
            cell: None,
            cell_field,
            ribbon,
        }
    }
}
