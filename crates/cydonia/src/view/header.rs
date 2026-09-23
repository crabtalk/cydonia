//! The band across the top of the detail column: what the pane is showing, and
//! the `···` that acts on it.
//!
//! Pulled, not pushed: [`Cydonia::detail`] already builds the body, so it asks
//! the pane rather than having each register itself into a shared slot the way
//! `../desktop` must. Two panes showing at once is this called twice — see
//! `todos/panels.md`.

use crate::model::workspace::Showing;
use crate::view::{
    component::menu::Menu,
    leaf::Pane,
    root::{self, Cydonia, ToggleChanges},
    sidebar::{Renaming, Row},
};
use bezel::{
    gpui::{AnyElement, App, Context, FontWeight, SharedString, Window, div, prelude::*, px},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
/// What a pane puts in the band.
pub(crate) struct Toolbar {
    /// What the pane is showing, by the name the sidebar would list it under.
    pub title: String,
    pub number: Option<u64>,
    /// The entry that name belongs to. Every pane has one today; the field is
    /// an option because a pane standing in for something not yet made — see
    /// [`Cydonia::launch`] — has a band and nothing to act on.
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
    /// In the band, in place. See [`Cydonia::header_renaming`].
    Inline(Renaming),
    /// In the board's identity panel, by id: a board is named by its name *and*
    /// its key, so one place edits both. See [`Cydonia::toggle_info`].
    Panel(String),
    /// In the page itself: an article's title is the first line of the
    /// document, so the pane already holds the field — see
    /// [`crate::view::article`]. Nothing for the band to draw or offer.
    Page,
}

impl Cydonia {
    /// What the pane showing in `pane` puts in the band. Takes the pane rather
    /// than reading [`Cydonia::showing`], so a second panel is a second call.
    pub(crate) fn toolbar(&self, pane: Pane, cx: &App) -> Option<Toolbar> {
        let workspace = self.workspace.read(cx);
        let project = workspace.active?;
        let open = workspace.projects.get(project)?;
        let showing = match pane {
            Pane::Chat => Showing::Session(open.active?),
            Pane::Board => Showing::Board(open.board?),
            Pane::Article => Showing::Article(open.article?),
            Pane::Table => Showing::Table(open.table?),
        };
        self.toolbar_of(project, showing, cx)
    }

    /// The same, for an entry named outright rather than read off the
    /// project's selection — what a pane of a space puts in its own bar,
    /// which is some entry other than the one in front.
    pub(crate) fn toolbar_of(&self, project: usize, showing: Showing, cx: &App) -> Option<Toolbar> {
        let workspace = self.workspace.read(cx);
        let open = workspace.projects.get(project)?;
        Some(match showing {
            Showing::Session(id) => {
                let chat = open.session(id)?;
                Toolbar {
                    title: chat.label(),
                    number: chat.number,
                    entry: Some(Entry {
                        row: Row::Session { project, id },
                        archived: chat.closed,
                        naming: Naming::Inline(Renaming::Session(id)),
                    }),
                }
            }
            Showing::Board(ix) => {
                let board = open.boards.get(ix)?;
                Toolbar {
                    title: board.label().to_owned(),
                    number: board.number,
                    entry: Some(Entry {
                        row: Row::Board { project, ix },
                        archived: board.archived,
                        naming: Naming::Panel(board.id.clone()),
                    }),
                }
            }
            Showing::Article(ix) => {
                let article = open.articles.get(ix)?;
                Toolbar {
                    title: article.label().to_owned(),
                    number: article.number,
                    entry: Some(Entry {
                        row: Row::Article { project, ix },
                        archived: article.archived,
                        naming: Naming::Page,
                    }),
                }
            }
            Showing::Table(ix) => {
                let table = open.tables.get(ix)?;
                Toolbar {
                    title: table.name.clone(),
                    number: table.number,
                    entry: Some(Entry {
                        row: Row::Table { project, ix },
                        archived: table.archived,
                        naming: Naming::Inline(Renaming::Table(table.key.clone())),
                    }),
                }
            }
        })
    }

    /// Which rename the band is drawing the name field for. One field, one
    /// place: the sidebar row asks this before drawing it, and the band wins
    /// because it is what is left when the sidebar is folded away.
    pub(crate) fn header_renaming(&self, cx: &App) -> Option<&Renaming> {
        // A space draws no band, so there is no field here to be the one
        // place — the sidebar row draws it instead. Without this the field
        // would be nowhere: the row stands down for a band that is not there.
        if self.arranged(cx) {
            return None;
        }
        let at = self.renaming.as_ref()?;
        let showing = self.toolbar(self.showing(cx)?, cx)?;
        let Naming::Inline(shown) = showing.entry?.naming else {
            return None;
        };
        (shown == *at).then_some(at)
    }

    /// The band, drawn over the pane rather than above it — the composer sits
    /// on the other edge the same way.
    pub(crate) fn pane_header(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let toolbar = self.showing(cx).and_then(|pane| self.toolbar(pane, cx));
        // The lights belong to the window, not a pane, so their clearance is
        // taken here and nowhere a pane can see it.
        let inset = match self.sidebar_open || window.is_fullscreen() {
            true => root::HEADER_INSET,
            false => root::TOOLBAR_INSET,
        };
        let renaming = self.header_renaming(cx).is_some();
        // Which board the band is showing, if any — the only entry with a
        // panel.
        let board = toolbar.as_ref().and_then(|toolbar| match &toolbar.entry {
            Some(Entry {
                naming: Naming::Panel(id),
                ..
            }) => Some(id.clone()),
            _ => None,
        });
        root::band()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .gap(px(8.))
            // Past the lights, which are the window's and are drawn over
            // whatever is at its top left.
            .pl(px(inset))
            // Above the pane, which runs under it.
            // The fold belongs to whichever column runs along the window's
            // left edge, so with the sidebar gone it is this one's.
            .children((!self.sidebar_open).then(|| self.fold_toggle(cx).into_any_element()))
            .children(toolbar.map(|toolbar| {
                match renaming {
                    true => div().flex_1().min_w_0().child(self.name_field(cx)),
                    false => div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        // A board's name opens its identity panel; the other
                        // panes have one field and rename from the `···`.
                        .child(
                            div()
                                .relative()
                                .min_w_0()
                                .flex_1()
                                // A row, so the name is only as wide as it
                                // reads — stretched, it would swallow presses
                                // on the empty half of the band.
                                .flex()
                                .flex_row()
                                .items_center()
                                .child(
                                    div()
                                        .id("header-title")
                                        .min_w_0()
                                        .truncate()
                                        .line_clamp(1)
                                        .text_style(TextStyle::Subheadline)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .when_some(board.clone(), |title, _| title.cursor_pointer())
                                        .child(toolbar.title)
                                        .when_some(board.clone(), |title, id| {
                                            // Noted on the way down, ahead of
                                            // the panel's own dismiss — see
                                            // [`Cydonia::toggle_info`].
                                            let opened = id.clone();
                                            title
                                                .capture_any_mouse_down(cx.listener(
                                                    move |this, _, _, _| {
                                                        this.info_pressed =
                                                            this.info.as_ref().is_some_and(
                                                                |info| info.board == opened,
                                                            );
                                                    },
                                                ))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.toggle_info(&id, window, cx);
                                                    },
                                                ))
                                        }),
                                )
                                .children(toolbar.number.map(|number| {
                                    div()
                                        .id("header-entry-number")
                                        .flex_none()
                                        .ml(px(6.))
                                        .text_style(TextStyle::Caption)
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(theme.text_muted)
                                        .child(format!("#{number}"))
                                }))
                                .children(board.as_deref().and_then(|id| self.info_panel(id, cx))),
                        )
                        .children(toolbar.entry.map(|entry| {
                            self.menu_button(
                                SharedString::from("header-menu"),
                                // On show: hiding a trigger until the pointer
                                // finds it is what a list of rows needs, not a
                                // band with one control.
                                None,
                                icons::layout::Ellipsis,
                                Menu::Header,
                                cx,
                            )
                            .children(self.entry_menu(
                                Menu::Header,
                                entry.row,
                                entry.archived,
                                cx,
                            ))
                        })),
                }
            }))
            .children(
                // Nothing to open it on where there is no directory in front
                // — see [`Cydonia::shell_cwd`].
                (!self.changes_open && self.shell_cwd(cx).is_some())
                    .then(|| self.changes_toggle(cx)),
            )
            .into_any_element()
    }

    /// The control that brings the right panel back, on the band rather than
    /// on the panel: the panel's own hide button goes down with the panel. Only
    /// drawn while the panel is closed. Mirrors [`Cydonia::fold_toggle`].
    fn changes_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        theme
            .icon_button(
                icons::layout::PanelRight,
                ButtonStyle::Ghost,
                Some(Fade::new(Painter::of(cx), "toggle-changes")),
            )
            .id("toggle-changes")
            .flex_none()
            .tooltip(|window, cx| Tooltip::text("Show right panel", window, cx))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleChanges), cx))
    }
}
