//! What a tab names its entry by, and the window's band over the screens that
//! stand where the panes would be.

use crate::model::workspace::Showing;
use crate::view::{
    component::panel::changes_toggle,
    sidebar::Row,
    window::chrome,
    window::root::{self, Cydonia},
};
use artifact::space::Member;
use bezel::{
    gpui::{AnyElement, App, Context, FontWeight, Window, div, prelude::*, px},
    motion::Painter,
    theme::{TextStyle, Theme, Typeset},
    ui::titlebar::CaptionSide,
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
            project: open.path.clone(),
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
    /// library, or the launch view: past the lights or the left caption
    /// buttons, the fold while the sidebar is shut, the library's name, the
    /// right panel's toggle and the right caption buttons.
    pub(crate) fn window_band(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let right = chrome::has(CaptionSide::Right, window, cx);
        // The window's top left is the sidebar's while it is open.
        let left = !self.sidebar_open && chrome::has(CaptionSide::Left, window, cx);
        // The lights belong to the window, not a pane, so their clearance is
        // taken here and nowhere a pane can see it.
        let inset = match self.sidebar_open {
            true => root::HEADER_INSET,
            false => root::toolbar_inset(window, cx),
        };
        let title = self.library.is_some().then(|| {
            div()
                .min_w_0()
                .truncate()
                .text_style(TextStyle::Subheadline)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child("Library")
        });
        root::band()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .gap(px(8.))
            // Past the lights, which are the window's and are drawn over
            // whatever is at its top left.
            .pl(px(if left { 0. } else { inset }))
            .when(right, |band| band.pr_0())
            .children(
                left.then(|| chrome::caption(CaptionSide::Left, window, cx))
                    .flatten(),
            )
            // The fold belongs to whichever column runs along the window's
            // left edge, so with the sidebar gone it is this one's.
            .children((!self.sidebar_open).then(|| self.fold_controls(cx)))
            .children(title)
            .child(chrome::grip("header-grip", &self.drag, window))
            .children(
                // Nothing to open it on where there is no directory in front
                // — see [`Cydonia::shell_cwd`].
                self.shell_cwd(cx).is_some().then(|| {
                    changes_toggle(
                        match self.changes_open {
                            true => "Hide right panel",
                            false => "Show right panel",
                        },
                        Painter::of(cx),
                        Theme::of(cx),
                    )
                }),
            )
            .children(
                right
                    .then(|| chrome::caption(CaptionSide::Right, window, cx))
                    .flatten(),
            )
            .into_any_element()
    }
}
