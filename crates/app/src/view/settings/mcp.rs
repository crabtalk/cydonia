//! The MCP section: the servers the agents are offered, the form for one the
//! registry does not carry, and the registry itself.

use crate::{
    mcp::{self, McpServer},
    view::settings::{AddMcp, SearchMcp, SettingsWindow},
};
use bezel::{
    gpui::{
        AnyElement, Context, Div, Entity, Focusable as _, MouseButton, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        input::TextField,
        widgets::{ButtonStyle, Buttons, Content, Controls, Scaffolding, Status},
    },
};
use cacp_agents::mcp as registry;

/// A field in a card row. `TextField` paints its own frame and moves its own
/// caret, but nothing focuses it on a press — `ui::focus` binds `tab` and
/// stops there — so the surface holding it does, as the board does for the
/// card it opens.
fn field_slot(field: &Entity<TextField>, cx: &Context<SettingsWindow>) -> Div {
    let handle = field.read(cx).focus_handle(cx);
    div()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, window, cx| window.focus(&handle, cx)),
        )
        .child(field.clone())
}

impl SettingsWindow {
    /// Search the registry for whatever the field holds. Blocking and live —
    /// the catalog is thousands of entries answered server-side, so there is
    /// nothing to cache and nothing to filter locally.
    pub(super) fn search_mcp(&mut self, _: &SearchMcp, _: &mut Window, cx: &mut Context<Self>) {
        let query = self.search.read(cx).content().to_string();
        self.searching = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { registry::search(&query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.searching = false;
                match found {
                    Ok(servers) => this.results = Some(servers),
                    Err(err) => this.error = Some(format!("{err:#}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The hand-entered server. A name is required because it is what the
    /// agent calls the server; the address is read for which kind it is.
    pub(super) fn add_mcp(&mut self, _: &AddMcp, _: &mut Window, cx: &mut Context<Self>) {
        let name = self.add_name.read(cx).content().trim().to_owned();
        let address = self.add_address.read(cx).content().to_string();
        if name.is_empty() || address.trim().is_empty() {
            return;
        }
        self.wrote(mcp::put(mcp::from_address(name, &address)), cx);
        self.add_name.update(cx, |field, cx| field.clear(cx));
        self.add_address.update(cx, |field, cx| field.clear(cx));
    }

    /// Install a registry server and name it in the store. Blocking: an npm
    /// server is fetched here, so it runs off the UI thread like an agent's.
    fn install_mcp(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(server) = self
            .results
            .as_ref()
            .and_then(|found| found.get(ix))
            .cloned()
        else {
            return;
        };
        let key = server.id.clone();
        self.busy.insert(key.clone());
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { mcp::install(&server) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy.remove(&key);
                this.wrote(done, cx);
            });
        })
        .detach();
    }

    /// Every write ends the same way: surface a failure, and re-read the store
    /// rather than patch the copy the rows were drawn from.
    fn wrote(&mut self, done: anyhow::Result<()>, cx: &mut Context<Self>) {
        match done {
            Ok(()) => {
                self.error = None;
                self.reload();
            }
            Err(err) => self.error = Some(format!("{err:#}").into()),
        }
        cx.notify();
    }

    /// One configured server: what the agent will be told, and the two things
    /// you can do about it.
    fn mcp_row(
        &self,
        ix: usize,
        server: &McpServer,
        first: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let name = server.name.clone();
        let enabled = server.enabled;
        theme
            .card_row(first)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(server.name.clone()))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(11.5))
                            .font_family(theme.font_mono.clone())
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(server.address()),
                    ),
            )
            .children(server.id.is_none().then(|| theme.badge("custom")))
            .child(
                theme
                    .toggle(enabled)
                    .id(("mcp-enabled", ix))
                    .cursor_pointer()
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| {
                            let done = mcp::set_enabled(&name, !enabled);
                            this.wrote(done, cx);
                        }
                    })),
            )
            .child(
                theme
                    .button(
                        "Remove",
                        ButtonStyle::Ghost,
                        Some(Fade::new(painter, format!("mcp-remove-{ix}"))),
                    )
                    .id(("mcp-remove", ix))
                    .flex_none()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let done = mcp::remove(&name);
                        this.wrote(done, cx);
                    })),
            )
            .into_any_element()
    }

    /// One registry hit. Already configured, installable, or published in a
    /// form nothing here can launch.
    fn mcp_result_row(
        &self,
        ix: usize,
        server: &registry::Server,
        first: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let held = self
            .configured
            .iter()
            .any(|held| held.id.as_deref() == Some(server.id.as_str()));
        let busy = self.busy.contains(&server.id);
        theme
            .card_row(first)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(server.name.clone()))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(11.5))
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(match server.description.is_empty() {
                                true => server.id.clone(),
                                false => server.description.clone(),
                            }),
                    ),
            )
            .children(server.is_remote().then(|| theme.badge("remote")))
            .child(match (busy, held, server.installable()) {
                (true, _, _) => div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme.text_faint)
                    .child("working…")
                    .into_any_element(),
                (false, true, _) => div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme.text_faint)
                    .child("added")
                    .into_any_element(),
                (false, false, true) => theme
                    .button(
                        "Add",
                        ButtonStyle::Ghost,
                        Some(Fade::new(painter, format!("mcp-add-{ix}"))),
                    )
                    .id(("mcp-add", ix))
                    .flex_none()
                    .on_click(cx.listener(move |this, _, _, cx| this.install_mcp(ix, cx)))
                    .into_any_element(),
                // Published, in a form that cannot be launched yet.
                (false, false, false) => div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(theme.text_faint)
                    .child("unavailable")
                    .into_any_element(),
            })
            .into_any_element()
    }

    /// The MCP section: what the agents are being offered, the form for a
    /// server the registry does not carry, and the registry.
    pub(super) fn mcp_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);

        let configured = match self.configured.is_empty() {
            true => theme.group_box().child(
                theme.card_row(true).child(
                    div()
                        .text_size(px(12.5))
                        .text_color(theme.text_muted)
                        .child("No servers yet. Every agent is offered the ones you add here."),
                ),
            ),
            false => theme.group_box().children(
                self.configured
                    .iter()
                    .enumerate()
                    .map(|(ix, server)| self.mcp_row(ix, server, ix == 0, cx)),
            ),
        };

        let add = theme.group_box().child(
            theme
                .card_row(true)
                .child(field_slot(&self.add_name, cx).flex_none().w(px(160.)))
                .child(field_slot(&self.add_address, cx).flex_1().min_w_0())
                .child(
                    theme
                        .button(
                            "Add",
                            ButtonStyle::Ghost,
                            Some(Fade::new(painter, "mcp-put")),
                        )
                        .id("mcp-put")
                        .flex_none()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_mcp(&AddMcp, window, cx);
                        })),
                ),
        );

        let mut search = theme.group_box().child(
            theme
                .card_row(true)
                .child(field_slot(&self.search, cx).flex_1().min_w_0())
                .child(
                    theme
                        .button(
                            match self.searching {
                                true => "Searching…",
                                false => "Search",
                            },
                            ButtonStyle::Ghost,
                            Some(Fade::new(painter, "mcp-search")),
                        )
                        .id("mcp-search")
                        .flex_none()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search_mcp(&SearchMcp, window, cx);
                        })),
                ),
        );
        if let Some(results) = self.results.as_ref() {
            search = match results.is_empty() {
                true => search.child(
                    theme.card_row(false).child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.text_muted)
                            .child("Nothing matched."),
                    ),
                ),
                false => search.children(
                    results
                        .iter()
                        .enumerate()
                        .map(|(ix, server)| self.mcp_result_row(ix, server, false, cx)),
                ),
            };
        }

        div()
            .flex()
            .flex_col()
            .children(self.error.clone().map(|err| theme.error_strip(err)))
            .child(configured)
            .child(theme.field_label("Add a server").mt(px(24.)))
            .child(add)
            .child(theme.field_label("Registry").mt(px(24.)))
            .child(search)
            .into_any_element()
    }

    /// Re-read the store. The file is the truth, so every write and every way
    /// back into the section arrives here rather than patching the drawn copy.
    pub(super) fn reload(&mut self) {
        self.configured = mcp::servers();
    }
}
