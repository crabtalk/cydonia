//! What a tab names its entry by, the library's title row, and the window's
//! band over the launch view.

use crate::{
    model::workspace::{Member, Showing},
    view::{
        kit::{menu, title::Title},
        sidebar::Row,
        window::{
            chrome,
            root::{self, Cydonia},
        },
    },
};
use bezel::{
    gpui::{AnyElement, App, Context, Window, prelude::*, px},
    theme::Theme,
    ui::{icons, tooltip::Tooltip, widgets::Content as _},
};

/// What a tab puts in its strip and its title row.
pub(crate) struct Toolbar {
    /// What the tab is showing, by the name the sidebar would list it under.
    pub title: String,
    pub number: Option<u64>,
    /// The entry that name belongs to. Nothing for an app, which has nothing
    /// to act on.
    pub entry: Option<Entry>,
}

/// The thing the title names: what to put a menu on, whether that menu offers
/// Archive or Unarchive, and how the thing is renamed.
pub(crate) struct Entry {
    pub row: Row,
    pub archived: bool,
    pub naming: Naming,
}

/// Where an entry is renamed — one place per kind, never two.
pub(crate) enum Naming {
    /// In the title row's field. See [`crate::view::pane::Tab::start_rename`].
    Inline,
    /// In the board's identity panel, by id: a board is named by its name *and*
    /// its key, so one place edits both. See
    /// [`crate::view::pane::Tab::toggle_info`].
    Panel(String),
    /// In the page itself: an article's title is the first line of the
    /// document, so the tab already holds the field — see
    /// [`crate::view::article`]. Nothing for the title row to draw or offer.
    Page,
}

impl Cydonia {
    /// What the tab on `member` puts in its strip and its title row.
    pub(crate) fn member_toolbar(&self, member: &Member, cx: &App) -> Option<Toolbar> {
        match member {
            Member::App { app } => self.app_toolbar(app, cx),
            Member::Entry(_) => {
                let (project, showing) = self.workspace.read(cx).showing_of(member, cx)?;
                self.toolbar_of(project, showing, cx)
            }
        }
    }

    /// What a tab showing the app `id` puts in its title row: the app's name.
    /// An app has no number and nothing to rename.
    fn app_toolbar(&self, id: &str, cx: &App) -> Option<Toolbar> {
        let program = self.workspace.read(cx).program(id, cx)?;
        Some(Toolbar {
            title: program.read(cx).label(),
            number: None,
            entry: None,
        })
    }

    /// The same, for an entry named by its project and what it shows.
    fn toolbar_of(&self, project: usize, showing: Showing, cx: &App) -> Option<Toolbar> {
        let workspace = self.workspace.read(cx);
        let open = workspace.projects.get(project)?;
        let row = Row::Entry {
            project: open.id,
            showing: showing.clone(),
        };
        let facts = open.facts(&showing, cx)?;
        let naming = match showing {
            Showing::Session(_) => Naming::Inline,
            Showing::Board(id) => Naming::Panel(id),
            Showing::Article(_) => Naming::Page,
            Showing::Table(_) => Naming::Inline,
        };
        Some(Toolbar {
            title: facts.label,
            number: facts.number,
            entry: Some(Entry {
                row,
                archived: facts.archived,
                naming,
            }),
        })
    }

    /// The window's band over the launch view: the room the window's corner
    /// controls stand in, and the grip.
    pub(crate) fn window_band(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // The window's top left is the sidebar's while it is open.
        let corners = chrome::Corners {
            left: !self.sidebar_open,
            right: true,
            tabs: self.workspace.read(cx).settings.appearance.global_tabs(),
        };
        chrome::top_band(corners.tabs)
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .gap(px(8.))
            .pl(px(corners.lead(root::HEADER_INSET, window, cx)))
            .pr(px(corners.trail(root::BUTTON_EDGE, window, cx)))
            .child(chrome::grip("header-grip", &self.drag, window))
            .into_any_element()
    }

    /// The library's title row: its mark and name, and the `×` that closes
    /// it.
    pub(crate) fn library_title(
        &self,
        heading: Title,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let mark = theme
            .icon_deck(icons::text::LibraryBig, 0)
            .mr(px(6.))
            .into_any_element();
        let label = heading.label(mark, Title::name(&theme).child("Library"), &theme);
        let grip = heading
            .edge
            .top
            .then(|| chrome::grip("library-grip", &self.drag, window));
        let close = menu::button(
            "library-close",
            icons::notifications::X,
            heading.reveal(),
            false,
            cx,
        )
        .tooltip(|window, cx| Tooltip::text("Close", window, cx))
        .on_click(cx.listener(|this, _, window, cx| {
            cx.stop_propagation();
            this.close_library(window, cx);
        }));
        heading
            .row(window, cx)
            .child(heading.slot().child(label))
            .children(grip)
            .child(close)
            .into_any_element()
    }
}
