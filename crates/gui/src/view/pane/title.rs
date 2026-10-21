//! A pane's title row, over what it shows: its kind's mark and what it is on,
//! the field that renames it, a board's identity panel, an article's
//! comments, the `···` holding the entry's menu, the move and the expand of
//! a pane beside others, and the `×` that closes it.

use super::{Tab, TabMenu, header::Naming};
use crate::view::{
    kit::{icon, menu},
    pane,
    sidebar::Renaming,
    window::{entry::Host, reach::InWindow as _, root},
};
use bezel::{
    gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::Case,
        tooltip::Tooltip,
    },
};

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
        // The focused pane's tools are on show, another's under the pointer.
        let reveal = match self.focused {
            true => menu::Reveal::Always,
            false => menu::Reveal::Group(pane::HOVER_GROUP),
        };
        let comments = self
            .article_shown(cx)
            .map(|view| view.read(cx).comments.clone());
        let row = div()
            .flex_none()
            .h(px(root::CARD_ROW_HEIGHT))
            .pl(px(root::HEADER_INSET))
            .pr(px(root::CARD_EDGE))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.));
        if self.renaming.is_some() {
            return Some(
                row.child(div().flex_1().min_w_0().child(self.name_field.clone()))
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
        let name = div()
            .relative()
            .min_w_0()
            .flex_1()
            // A row, so the name is only as wide as it reads — stretched, it
            // would swallow presses on the empty half of the row.
            .flex()
            .flex_row()
            .items_center()
            .child(
                icons::icon(icon::entry_icon(&self.entry, self.workspace.read(cx), cx))
                    .flex_none()
                    .mr(px(6.))
                    .size(px(14.))
                    .text_color(theme.text_muted),
            )
            .child(title)
            .children(toolbar.number.map(|number| {
                div()
                    .flex_none()
                    .ml(px(6.))
                    .text_style(TextStyle::Caption)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.text_muted)
                    .child(format!("#{number}"))
            }))
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
                    let tab = cx.entity();
                    // From the window: its rows read this tab back.
                    this.defer_window(window, cx, move |root, window, cx| {
                        let rows = Self::through(root.entry_rows(host, &row, archived, cx));
                        let rows = std::cell::RefCell::new(Some(rows));
                        tab.update(cx, |this, cx| {
                            ContextMenu::toggle(
                                this,
                                |this| &mut this.menu,
                                TabMenu::Entry,
                                Place::BelowEnd,
                                move |_, _, _| {
                                    menu::chooser(rows.borrow_mut().take().unwrap_or_default())
                                },
                                window,
                                cx,
                            )
                        });
                    });
                }));
            ContextMenu::trigger(button, |this: &mut Self| &mut this.menu, TabMenu::Entry, cx)
                .children(self.menu.render_for(
                    &TabMenu::Entry,
                    "tab-entry-menu-card",
                    |this| &mut this.menu,
                    window,
                    cx,
                ))
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
                        let (entry, tab) = (this.entry.clone(), cx.entity());
                        // From the window: its rows read this tab back.
                        this.defer_window(window, cx, move |root, window, cx| {
                            let rows = Self::through(root.move_rows(&entry, cx));
                            let rows = std::cell::RefCell::new(Some(rows));
                            tab.update(cx, |this, cx| {
                                ContextMenu::toggle(
                                    this,
                                    |this| &mut this.menu,
                                    TabMenu::Move,
                                    Place::BelowEnd,
                                    move |_, _, _| {
                                        menu::chooser(rows.borrow_mut().take().unwrap_or_default())
                                    },
                                    window,
                                    cx,
                                )
                            });
                        });
                    }));
                Some(
                    ContextMenu::trigger(
                        button,
                        |this: &mut Self| &mut this.menu,
                        TabMenu::Move,
                        cx,
                    )
                    .children(self.menu.render_for(
                        &TabMenu::Move,
                        "tab-move-menu-card",
                        |this| &mut this.menu,
                        window,
                        cx,
                    )),
                )
            }
        };
        // In a space it closes the pane; alone, the tab with it.
        let close = menu::button("tab-close-pane", icons::notifications::X, reveal, false, cx)
            .tooltip(|window, cx| Tooltip::text("Close", window, cx))
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                let entry = this.entry.clone();
                this.defer_window(window, cx, move |root, window, cx| {
                    root.close_pane(&entry, window, cx)
                });
            }));
        Some(
            row.child(name)
                .children(comments)
                .children(menu)
                .children(shift)
                .children(zoom)
                .child(close)
                .into_any_element(),
        )
    }

    /// Put the row's field on `what`.
    pub(crate) fn start_rename(
        &mut self,
        what: Renaming,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = what.label(self.workspace.read(cx), cx);
        self.name_field
            .update(cx, |field, cx| field.start(label, Case::Mixed, window, cx));
        // One editor at a time — see [`Tab::open_info`].
        self.info = None;
        self.renaming = Some(what);
        cx.notify();
    }

    pub(crate) fn commit_name(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(what) = self.renaming.take() else {
            return;
        };
        self.workspace
            .update(cx, |workspace, cx| what.commit(name, workspace, cx));
        cx.notify();
    }

    pub(crate) fn dismiss_name(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }
}
