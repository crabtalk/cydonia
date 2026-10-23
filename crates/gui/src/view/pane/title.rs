//! A pane's title row, over what it shows: its kind's mark and what it is on,
//! the field that renames it, a board's identity panel, an article's
//! comments, the `···` holding the entry's menu, the move and the expand of
//! a pane beside others, and the `×` that closes it. Floating, it stands on
//! the chrome over the pane's card. Along the window's top edge, while the
//! window has no top row of its own, it is that row at the pane's place —
//! see [`crate::view::pane::Edge`].

use super::{Tab, TabMenu, header::Naming};
use crate::view::{
    kit::{
        icon,
        menu::{self, MenuHost as _},
        rename::Renaming,
    },
    window::{chrome, entry::Host, reach::InWindow as _, root},
};
use artifact::reference::Reference;
use bezel::{
    gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::Place,
        icons,
        tabs::{self, State},
        tooltip::Tooltip,
        widgets::Content as _,
    },
};

/// The hover group a title row is, which its tools show on — the ones other
/// views hang in it included.
pub(crate) const TITLE_GROUP: &str = "pane-title";

impl Tab {
    /// The row: the entry's name, or the field renaming it, and what acts on
    /// it. Nothing for an entry that has gone.
    pub(crate) fn title_row(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let toolbar = self
            .root
            .upgrade()?
            .read(cx)
            .member_toolbar(&self.entry, cx)?;
        let board = toolbar
            .entry
            .as_ref()
            .and_then(|entry| match &entry.naming {
                Naming::Panel(id) => Some(id.clone()),
                Naming::Inline | Naming::Page => None,
            });
        // The first line of that board's description, which its name shows on
        // hover.
        let described: Option<SharedString> = board.as_deref().and_then(|id| {
            let board = self.workspace.read(cx).board_at(id, cx)?;
            let line = board
                .description
                .lines()
                .find(|line| !line.trim().is_empty())?;
            Some(line.trim().to_owned().into())
        });
        let comments = self
            .article_shown(cx)
            .map(|view| view.read(cx).comments.clone());
        let edge = self.edge;
        let corners = chrome::Corners {
            left: edge.lights,
            right: edge.toggle,
            tabs: false,
        };
        let floating = self.workspace.read(cx).settings.appearance.floating;
        // Flat, the title stands on a tab's chip, the focused pane's lit.
        let chip = (!floating).then(|| match self.focus.contains_focused(window, cx) {
            true => State::Focused,
            false => State::Front,
        });
        let reveal = match chip {
            Some(State::Focused) => menu::Reveal::Always,
            _ => menu::Reveal::Group(TITLE_GROUP),
        };
        // The pane stands in from the window's edges by the column's padding.
        // A chip's inset and ring slot stand out into it, so its text lines up
        // with the content under it.
        let on_chrome = floating && edge.top;
        let chip_inset = match chip {
            Some(_) => tabs::TAB_PAD + 1.,
            None => 0.,
        };
        let lead = match corners.left {
            true => corners.lead(0., window, cx) - root::card_gap(floating),
            false if on_chrome => root::CARD_EDGE,
            false => root::HEADER_INSET,
        } - chip_inset;
        let trail = match corners.right {
            true => corners.trail(0., window, cx) - root::card_gap(floating),
            false => root::CARD_EDGE,
        };
        let row = div()
            .group(TITLE_GROUP)
            .flex_none()
            .h(px(root::CARD_ROW_HEIGHT))
            .pl(px(lead))
            .pr(px(trail))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.));
        if self.rename.what.is_some() {
            return Some(
                row.child(div().flex_1().min_w_0().child(self.rename.field.clone()))
                    .into_any_element(),
            );
        }
        let title = div()
            .id("tab-title")
            .min_w_0()
            .truncate()
            .line_clamp(1)
            .text_style(TextStyle::Subheadline)
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text)
            .when_some(described, |title, line| {
                title.tooltip(move |window, cx| Tooltip::text(line.clone(), window, cx))
            })
            .child(toolbar.title)
            // A board's name opens its identity panel; the other kinds have
            // one field and rename from the `···`.
            .when_some(board.clone(), |title, id| {
                // Noted on the way down, ahead of the panel's own dismiss —
                // see [`Tab::toggle_info`].
                let opened = id.clone();
                title
                    .cursor_pointer()
                    .capture_any_mouse_down(cx.listener(move |this, _, _, _| {
                        this.info_pressed =
                            this.info.as_ref().is_some_and(|info| info.board == opened);
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_info(&id, window, cx);
                    }))
            });
        let label = match chip {
            Some(state) => tabs::chip(&theme, state).gap_0(),
            None => div().flex().flex_row().items_center(),
        }
        .min_w_0()
        .child(self.deck(window, cx))
        .child(title)
        .children(toolbar.number.map(|number| {
            div()
                .flex_none()
                .ml(px(6.))
                .text_style(TextStyle::Caption)
                .font_weight(FontWeight::NORMAL)
                .text_color(theme.text_muted)
                .child(Reference::entry(None, number, None).to_string())
        }));
        let name = div()
            .relative()
            .min_w_0()
            .when(!edge.top, |name| name.flex_1())
            // A row, so the name is only as wide as it reads — stretched, it
            // would swallow presses on the empty half of the row.
            .flex()
            .flex_row()
            .items_center()
            .child(label)
            .children(board.as_deref().and_then(|id| self.info_panel(id, cx)));
        let menu = toolbar.entry.map(|entry| {
            let open = self.menu.key() == Some(&TabMenu::Entry);
            let button = menu::button("tab-entry-menu", icons::layout::Ellipsis, reveal, open, cx)
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    let (row, archived, host) = (
                        entry.row.clone(),
                        entry.archived,
                        Host::Tab(this.entry.clone()),
                    );
                    this.window_menu(
                        |this| &mut this.menu,
                        TabMenu::Entry,
                        Place::BelowEnd,
                        move |root, _, cx| root.entry_rows(host, &row, archived, cx),
                        window,
                        cx,
                    );
                }));
            self.menu_trigger(button, TabMenu::Entry, cx)
                .children(self.menu_here(&TabMenu::Entry, "tab-entry-menu-card", window, cx))
        });
        // In a space beside another pane: it stands over them, or goes back.
        let zoom = self
            .root
            .upgrade()
            .and_then(|root| root.read(cx).zoomed(&self.entry, cx))
            .map(|zoomed| {
                let (mark, tip) = match zoomed {
                    true => (icons::arrows::Minimize2, "Restore"),
                    false => (icons::arrows::Maximize2, "Expand"),
                };
                menu::button("tab-zoom-pane", mark, reveal, false, cx)
                    .tooltip(move |window, cx| Tooltip::text(tip, window, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        let entry = this.entry.clone();
                        this.defer_window(window, cx, move |root, _, cx| {
                            root.toggle_zoom(&entry, cx)
                        });
                    }))
            });
        // Across a seam: the one way there is, or a menu of them.
        let moves = self
            .root
            .upgrade()
            .map(|root| root.read(cx).pane_moves(&self.entry, cx))
            .unwrap_or_default();
        let shift = match moves.as_slice() {
            [] => None,
            [(side, label, mark)] => {
                let (side, label) = (*side, *label);
                Some(
                    menu::button("tab-move-pane", *mark, reveal, false, cx)
                        .tooltip(move |window, cx| Tooltip::text(label, window, cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            let entry = this.entry.clone();
                            this.defer_window(window, cx, move |root, window, cx| {
                                root.move_pane(&entry, side, window, cx)
                            });
                        })),
                )
            }
            _ => {
                let open = self.menu.key() == Some(&TabMenu::Move);
                let button = menu::button("tab-move-pane", icons::arrows::Move, reveal, open, cx)
                    .tooltip(|window, cx| Tooltip::text("Move", window, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        let entry = this.entry.clone();
                        this.window_menu(
                            |this| &mut this.menu,
                            TabMenu::Move,
                            Place::BelowEnd,
                            move |root, _, cx| root.move_rows(&entry, cx),
                            window,
                            cx,
                        );
                    }));
                Some(
                    self.menu_trigger(button, TabMenu::Move, cx)
                        .children(self.menu_here(&TabMenu::Move, "tab-move-menu-card", window, cx)),
                )
            }
        };
        // In a space it closes the pane; alone, the tab with it — none while
        // the sidebar lists the spaces, which draws no tabs.
        let workspace = self.workspace.read(cx);
        let alone = workspace.active_space().is_none();
        let close = !(alone && workspace.settings.appearance.spaces_in_sidebar);
        let close = close.then(|| {
            menu::button("tab-close-pane", icons::notifications::X, reveal, false, cx)
                .tooltip(|window, cx| Tooltip::text("Close", window, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    let entry = this.entry.clone();
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.close_pane(&entry, window, cx)
                    });
                }))
        });
        // Along the window's top, the empty stretch the window is dragged by.
        let grip = match (edge.top, self.root.upgrade()) {
            (true, Some(root)) => Some(chrome::grip("pane-grip", &root.read(cx).drag, window)),
            _ => None,
        };
        Some(
            row.child(name)
                .children(grip)
                .children(comments)
                .children(menu)
                .children(shift)
                .children(zoom)
                .children(close)
                .into_any_element(),
        )
    }

    /// The pane's stack as a deck, its front entry's mark on top, opening the
    /// stack's list on a click.
    fn deck(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let behind = workspace.stack_of(&self.entry).len().saturating_sub(1);
        let deck = theme
            .icon_deck(icon::card_icon(&self.entry, workspace, cx), behind)
            .mr(px(6.));
        if behind == 0 {
            return deck.into_any_element();
        }
        let button = div()
            .id("tab-stack")
            .cursor_pointer()
            .child(deck)
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                let entry = this.entry.clone();
                this.window_menu(
                    |this| &mut this.menu,
                    TabMenu::Stack,
                    Place::Below,
                    move |root, _, cx| root.stack_rows(&entry, cx),
                    window,
                    cx,
                );
            }));
        self.menu_trigger(button, TabMenu::Stack, cx)
            .children(self.menu_here(&TabMenu::Stack, "tab-stack-menu-card", window, cx))
            .into_any_element()
    }

    /// Put the row's field on `what`.
    pub(crate) fn start_rename(
        &mut self,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rename.start(what, window, cx);
        // One editor at a time — see [`Tab::open_info`].
        self.info = None;
        cx.notify();
    }
}
