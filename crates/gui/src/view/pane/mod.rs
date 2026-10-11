//! Panes: their arrangement in a space, and what every pane carries.

pub mod arrangement;
pub mod drawer;
pub mod find;
pub mod footer;
pub mod header;
pub mod items;

use crate::{
    model::workspace::{SessionChanged, Showing, Workspace},
    view::{
        article,
        board::{self, DismissFind},
        component::{composer::Composer, ribbon::Ribbon, transcript},
        table,
        window::root::Cydonia,
    },
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        AnyElement, App, Context, Entity, FocusHandle, Render, WeakEntity, Window, div, prelude::*,
    },
    theme::Theme,
    ui::{
        context_menu::ContextMenu,
        icons,
        input::{FieldEvent, TextField},
        widgets::Content as _,
    },
};
use std::rc::Rc;

/// Which of a pane's menus is up.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum PaneMenu {
    /// The block picker on the article ribbon — see
    /// [`crate::view::component::ribbon`].
    Turn,
}

/// One pane: what it shows, and what it holds while it shows it.
pub(crate) struct Pane {
    /// The entry this pane is on, named the way a space names its members —
    /// which project it is in and which of that project's things it is. That
    /// is what ties a pane to a member of the arrangement.
    ///
    /// Nothing for the pane a window with no space open shows: it is on
    /// whatever the project was last left on, and the project holds that.
    pub(crate) entry: Option<artifact::space::Member>,
    pub(crate) workspace: Entity<Workspace>,
    pub(crate) root: WeakEntity<Cydonia>,
    /// The board views' shared card previews and drag domain — see
    /// [`Cydonia::card_docs`] and [`Cydonia::board_sort`].
    pub(crate) card_docs: Rc<board::Docs>,
    pub(crate) board_sort: bezel::ui::drag::Domain<board::BoardRegion, board::BoardItem>,
    /// Tracked on the pane this leaf draws, so the pane is an ancestor of the
    /// focused element and the chords claimed on it are reached — an action
    /// runs only through the focused element's ancestors. Where the focus lands
    /// for a pane holding nothing to type into; a session's composer and a
    /// document's editor are inside the pane already.
    pub(crate) focus: FocusHandle,
    /// What kind of entry the pane is showing.
    pub(crate) kind: Kind,
    /// Whether this is the pane with the window's focus — see
    /// [`Cydonia::sync_focus`].
    pub(crate) focused: bool,
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
    /// What the pane's drawer holds, while it is up — see
    /// [`crate::view::pane::drawer`].
    pub(crate) drawer: Option<drawer::Drawer>,
    /// The pane's view of each board it has shown, so a board shown again
    /// is where this pane left it.
    pub(crate) boards: Vec<Entity<board::BoardView>>,
    /// The same for each article.
    pub(crate) articles: Vec<Entity<article::ArticleView>>,
    /// The same for each session.
    pub(crate) sessions: Vec<Entity<transcript::SessionView>>,
    /// The same for each table.
    pub(crate) tables: Vec<Entity<table::TableView>>,
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
    /// The workspace search applied, which the find stands for while the bar
    /// is down — see [`Cydonia::push_finds`].
    pub(crate) applied: Option<artifact::search::Query>,
    /// The formatting bar over the open document's selection, and the URL
    /// field it puts up — see [`crate::view::component::ribbon`].
    pub(crate) ribbon: Ribbon,
    pub(crate) menu: ContextMenu<Self, PaneMenu>,
}

impl Pane {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        entry: Option<artifact::space::Member>,
        workspace: Entity<Workspace>,
        root: WeakEntity<Cydonia>,
        card_docs: Rc<board::Docs>,
        board_sort: bezel::ui::drag::Domain<board::BoardRegion, board::BoardItem>,
        composer: Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Self {
        let find_field = board::find_field(cx);
        // What the pane draws is read off the model, and a cached pane draws
        // again only when told.
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&workspace, |this, _, event: &SessionChanged, cx| {
            if this.draws_session(event.0, cx) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&find_field, |this, _, event: &FieldEvent, cx| {
            if let FieldEvent::Changed(_) = event {
                this.find_changed(cx);
            }
        })
        .detach();
        Self {
            entry,
            workspace,
            root,
            card_docs,
            board_sort,
            focus: cx.focus_handle(),
            kind: Kind::Session,
            focused: false,
            composer,
            queued_galleries: Default::default(),
            asked_session: false,
            drawer: None,
            boards: Vec::new(),
            articles: Vec::new(),
            sessions: Vec::new(),
            tables: Vec::new(),
            find_field,
            finding: false,
            find_at: 0,
            find_pattern: false,
            applied: None,
            ribbon: Ribbon::new(cx),
            menu: ContextMenu::default(),
        }
    }
}

impl Pane {
    /// What the pane shows: its entry, or for the pane a window with no space
    /// open shows, what the project in front is pointed at for the pane's kind.
    pub(crate) fn showing(&self, cx: &App) -> Option<(usize, Showing)> {
        let workspace = self.workspace.read(cx);
        match &self.entry {
            Some(entry) => workspace.showing_of(entry),
            None => {
                let project = workspace.active?;
                let showing = workspace.projects.get(project)?.selected(self.kind)?;
                Some((project, showing))
            }
        }
    }

    /// Whether the pane draws anything of session `id`: the session it shows,
    /// or the thread under the article it shows.
    fn draws_session(&self, id: u64, cx: &App) -> bool {
        self.shown_session(cx) == Some(id)
            || self
                .thread_article(cx)
                .is_some_and(|path| self.workspace.read(cx).article_thread(&path) == Some(id))
    }

    /// Run `act` on the window once this pane is no longer being updated: what
    /// the window does in answer can reach back into the pane.
    pub(crate) fn defer_window(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        act: impl FnOnce(&mut Cydonia, &mut Window, &mut Context<Cydonia>) + 'static,
    ) {
        let root = self.root.clone();
        window.defer(cx, move |window, cx| {
            root.update(cx, |root, cx| act(root, window, cx)).ok();
        });
    }

    /// What the pane shows, drawn: the item, and what the pane hangs over it.
    fn body(
        &mut self,
        showing: &Showing,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match showing {
            Showing::Session(id) => self.conversation(Some(*id), cx),
            Showing::Board(_) => {
                let Some(view) = self.board_shown(cx) else {
                    return div().flex_1().into_any_element();
                };
                let pill = match cx.has_active_drag() {
                    true => None,
                    false => self.search_pill(cx),
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .flex_col()
                    .on_action(cx.listener(|this, _: &DismissFind, _, cx| this.dismiss_find(cx)))
                    .child(view)
                    .children(pill)
                    .into_any_element()
            }
            Showing::Article(_) => {
                let Some(view) = self
                    .article_shown(cx)
                    .filter(|view| view.read(cx).editor().is_some())
                else {
                    return div().flex_1().into_any_element();
                };
                let focused = self.focused;
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(view)
                    .children(self.search_pill(cx))
                    // Last, and floated over the document from where the
                    // selection ends — the bar is chrome the page runs under.
                    .children(self.ribbon_bar(focused, window, cx))
                    .into_any_element()
            }
            Showing::Table(_) => match self.table_shown(cx) {
                Some(view) => view.into_any_element(),
                None => div().flex_1().into_any_element(),
            },
        }
    }

    /// What the pane's drawer draws for what it holds.
    fn drawer_face(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<drawer::Face> {
        let drawer = self.drawer_shown(cx)?;
        match &drawer.peek {
            drawer::Peek::Card(view) => view
                .clone()
                .update(cx, |view, cx| view.card_face(window, cx)),
            drawer::Peek::Entry(entry) => {
                let (entry, scroll, on) =
                    (entry.clone(), drawer.scroll.clone(), self.entry.clone());
                self.root
                    .update(cx, |root, cx| {
                        root.entry_face(&entry, on.as_ref(), &scroll, window, cx)
                    })
                    .ok()
            }
        }
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_items(cx);
        let theme = Theme::of(cx).clone();
        let showing = self.showing(cx);
        let body = match &showing {
            // The entry has gone since the space named it. The pane says so
            // rather than standing empty: a blank pane reads as a bug, and the
            // space is about to drop the member anyway — see
            // [`crate::model::workspace::Workspace::prune_spaces`].
            None if self.entry.is_some() => theme
                .empty_state(
                    icons::files::File,
                    "This entry has gone",
                    "It was deleted after the space was made.",
                )
                .into_any_element(),
            None => match self.kind {
                Kind::Session => self.conversation(None, cx),
                _ => div().flex_1().into_any_element(),
            },
            Some((_, showing)) => self.body(showing, window, cx),
        };
        let session = match &showing {
            Some((_, Showing::Session(id))) => Some(*id),
            None if self.kind == Kind::Session => self.shown_session(cx),
            _ => None,
        };
        // A pane of a space always carries its composer; the window's one pane
        // stands the reason there in its place when there is nowhere to send.
        let reachable = self.entry.is_some() || self.workspace.read(cx).reachable();
        let footer = match (&showing, session) {
            (_, Some(id)) if reachable => Some(self.session_footer(Some(id), cx)),
            (_, Some(id)) => self
                .adrift_strip(Some(id), cx)
                .map(|strip| crate::view::detail::footer(strip, None).into_any_element()),
            (Some((_, Showing::Article(_))), None) => self.thread_footer(cx),
            _ => None,
        };
        let selection = match session.is_some() && self.focused {
            true => self.selection_bar(cx),
            false => None,
        };
        let drawer = match self.drawer_face(window, cx) {
            Some(face) => self.drawer_layer(face, cx),
            None => Vec::new(),
        };
        let composer = self.composer.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .size_full()
            .flex()
            .flex_col()
            .child(body)
            .children(footer)
            .children(selection)
            // The whole pane takes a dropped picture for the composer.
            .when(session.is_some() && reachable, |pane| {
                pane.on_drop(move |paths: &bezel::gpui::ExternalPaths, _, cx| {
                    composer.update(cx, |composer, cx| composer.drop_paths(paths, cx));
                })
                .child(crate::view::detail::drop_wash(&theme))
            })
            .children(drawer)
    }
}
