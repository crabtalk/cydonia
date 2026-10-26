//! What a tab names its entry by, and the window's band over the screens that
//! stand where the panes would be.

use crate::{
    model::workspace::{Member, Showing},
    view::{
        sidebar::Row,
        window::{
            chrome,
            root::{self, Cydonia},
        },
    },
};
use bezel::{
    gpui::{AnyElement, App, Context, FontWeight, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
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

    /// The window's band over what stands where the panes would be — the
    /// library, or the launch view: the library's name, between the room the
    /// window's corner controls stand in.
    pub(crate) fn window_band(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // The window's top left is the sidebar's while it is open.
        let corners = chrome::Corners {
            left: !self.sidebar_open,
            right: true,
            tabs: self.workspace.read(cx).settings.appearance.global_tabs(),
        };
        let title = self.library_shown(cx).then(|| {
            div()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Subheadline)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child("Library")
        });
        chrome::top_band(corners.tabs)
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .gap(px(8.))
            .pl(px(corners.lead(root::HEADER_INSET, window, cx)))
            .pr(px(corners.trail(root::BUTTON_EDGE, window, cx)))
            .children(title)
            .child(chrome::grip("header-grip", &self.drag, window))
            .into_any_element()
    }
}
