//! A pane's title row in a space, over what it shows: what it is on, the
//! field that renames it, a board's identity panel, an article's comments,
//! and the `···` holding the entry's menu and the pane's. A tab holding one
//! entry has no title row: its name and `···` are the strip's, and its comments
//! and board panel stand in its top right corner.

use super::{Tab, TabMenu, header::Naming};
use crate::model::workspace::Slot;
use crate::view::{
    kit::menu,
    sidebar::Renaming,
    window::{entry::Host, reach::InWindow as _, root},
};
use bezel::{
    gpui::{AnyElement, App, Context, FontWeight, SharedString, Window, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::Case,
        tooltip::Tooltip,
    },
};

impl Tab {
    /// Whether this tab is the strip's front tab on its own, not a pane of a
    /// space.
    pub(crate) fn lone(&self, cx: &App) -> bool {
        self.workspace.read(cx).front_slot() == Some(&Slot::Entry(self.entry.clone()))
    }

    /// A lone tab's corner, over its content: an article's comments and a
    /// board's identity panel, under the strip's `···` that opens the panel.
    pub(crate) fn corner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let comments = self
            .article_shown(cx)
            .map(|view| view.read(cx).comments.clone());
        let panel = self
            .info
            .as_ref()
            .map(|info| info.board.clone())
            .and_then(|board| self.info_panel(&board, Place::BelowEnd, cx));
        if comments.is_none() && panel.is_none() {
            return None;
        }
        Some(
            div()
                .absolute()
                .top_0()
                .right_0()
                .h(px(root::HEADER_HEIGHT))
                .px(px(root::HEADER_INSET))
                .flex()
                .flex_row()
                .items_center()
                .children(comments)
                .children(panel)
                .into_any_element(),
        )
    }

    /// The row: the entry's name, or the field renaming it, and what acts on
    /// it. Nothing for an entry that has gone, or for a lone tab.
    pub(crate) fn title_row(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.lone(cx) {
            return None;
        }
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
        let row = div()
            .flex_none()
            .h(px(root::HEADER_HEIGHT))
            .px(px(root::HEADER_INSET))
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
            .children(
                board
                    .as_deref()
                    .and_then(|id| self.info_panel(id, Place::Below, cx)),
            );
        let menu = toolbar.entry.map(|entry| {
            let open = self.menu.key() == Some(&TabMenu::Entry);
            let button = menu::button(
                "tab-entry-menu",
                icons::layout::Ellipsis,
                menu::Reveal::Always,
                open,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                let (row, archived, host) = (
                    entry.row.clone(),
                    entry.archived,
                    Host::Tab(this.entry.clone()),
                );
                let (member, tab) = (this.entry.clone(), cx.entity());
                // From the window: its rows read this tab back.
                this.defer_window(window, cx, move |root, window, cx| {
                    let rows = Self::through(menu::sections([
                        root.entry_rows(host, &row, archived, cx),
                        root.pane_rows(&member, cx),
                    ]));
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
        Some(
            row.child(name)
                .children(comments)
                .children(menu)
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
