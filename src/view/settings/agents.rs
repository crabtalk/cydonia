//! The agents section: what the registry publishes, what is on this machine,
//! and the install or removal in flight.

use crate::{
    agent::{self, Listing},
    view::settings::SettingsWindow,
};
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px, svg},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons, Content, Scaffolding, Status},
    },
};

impl SettingsWindow {
    /// Fetch the catalog and each agent's local state, off the UI thread.
    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let listings = cx
                .background_executor()
                .spawn(async move { agent::listings() })
                .await;
            let missing = listings.iter().any(|listing| listing.icon.is_none());
            let ok = this
                .update(cx, |this, cx| {
                    this.listings = Some(listings);
                    cx.notify();
                })
                .is_ok();
            // The marks are worth waiting for but not worth waiting on: the
            // list is already readable, so they arrive in a second pass.
            if !ok || !missing {
                return;
            }
            let listings = cx
                .background_executor()
                .spawn(async move {
                    agent::prefetch_icons();
                    agent::listings()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.listings = Some(listings);
                cx.notify();
            });
        })
        .detach();
    }

    /// Put the agent on disk and name it in `settings.toml`, then read that
    /// file back so the composer's list and the file cannot disagree.
    fn install(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(agent) = self
            .listings
            .as_ref()
            .and_then(|l| l.get(ix))
            .map(|listing| listing.agent.clone())
        else {
            return;
        };
        let id = agent.id.clone();
        self.busy.insert(id.clone());
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { agent::install(&agent) })
                .await;
            let _ = this.update(cx, |this, cx| this.settled(id, done, cx));
        })
        .detach();
    }

    fn remove(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(id) = self
            .listings
            .as_ref()
            .and_then(|l| l.get(ix))
            .map(|listing| listing.agent.id.clone())
        else {
            return;
        };
        self.busy.insert(id.clone());
        self.error = None;
        cx.notify();
        let settled = id.clone();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { agent::remove(&id) })
                .await;
            let _ = this.update(cx, |this, cx| this.settled(settled, done, cx));
        })
        .detach();
    }

    /// Both halves end the same way: stop showing the spinner, surface a
    /// failure, and re-read what is now true of the machine.
    fn settled(&mut self, id: String, done: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.busy.remove(&id);
        match done {
            Ok(()) => {
                self.workspace
                    .update(cx, |workspace, cx| workspace.reload_settings(cx));
                self.load(cx);
            }
            Err(err) => self.error = Some(format!("{err:#}").into()),
        }
        cx.notify();
    }

    /// One agent: its mark and name, what it is, and the one thing you can do
    /// about it.
    fn agent_row(
        &self,
        ix: usize,
        listing: &Listing,
        first: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let busy = self.busy.contains(&listing.agent.id);
        let installed = listing.installed.clone();
        theme
            .card_row(first)
            .child(
                div()
                    .flex_none()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(listing.icon.clone().map(|path| {
                        svg()
                            .path(path)
                            .size(px(16.))
                            .flex_none()
                            .text_color(theme.text_muted)
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(listing.agent.name.clone()))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(
                                listing
                                    .agent
                                    .description
                                    .clone()
                                    .unwrap_or_else(|| listing.agent.id.clone()),
                            ),
                    ),
            )
            .children(
                installed
                    .clone()
                    .map(|version| theme.badge(format!("v{version}"))),
            )
            .child(match (busy, installed, listing.agent.installable()) {
                (true, _, _) => div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_faint)
                    .child("working…")
                    .into_any_element(),
                (false, Some(_), _) => theme
                    .button(
                        "Remove",
                        ButtonStyle::Ghost,
                        Some(Fade::new(painter, format!("remove-{ix}"))),
                    )
                    .id(("remove", ix))
                    .flex_none()
                    .on_click(cx.listener(move |this, _, _, cx| this.remove(ix, cx)))
                    .into_any_element(),
                (false, None, true) => theme
                    .button(
                        "Install",
                        ButtonStyle::Ghost,
                        Some(Fade::new(painter, format!("install-{ix}"))),
                    )
                    .id(("install", ix))
                    .flex_none()
                    .on_click(cx.listener(move |this, _, _, cx| this.install(ix, cx)))
                    .into_any_element(),
                // Published, but with no build this machine can run.
                (false, None, false) => div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_faint)
                    .child("unavailable")
                    .into_any_element(),
            })
            .into_any_element()
    }

    /// The agents section: everything the registry publishes, installed first
    /// so what you already have is what you see.
    pub(super) fn agents_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(listings) = self.listings.as_ref() else {
            return theme
                .group_box()
                .child(
                    theme.card_row(true).child(
                        div()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text_muted)
                            .child("Reading the agent catalogue…"),
                    ),
                )
                .into_any_element();
        };
        if listings.is_empty() {
            return theme
                .empty_state(
                    icons::WIDGET,
                    "No catalogue",
                    "The agent registry could not be reached.",
                )
                .into_any_element();
        }
        let mut order: Vec<usize> = (0..listings.len()).collect();
        order.sort_by_key(|&ix| {
            (
                listings[ix].installed.is_none(),
                !listings[ix].agent.installable(),
                listings[ix].agent.name.to_lowercase(),
            )
        });
        let mut rows: Vec<AnyElement> = Vec::new();
        for (nth, ix) in order.into_iter().enumerate() {
            rows.push(self.agent_row(ix, &listings[ix], nth == 0, cx));
        }
        div()
            .flex()
            .flex_col()
            .children(self.error.clone().map(|err| theme.error_strip(err)))
            .child(theme.group_box().children(rows))
            .into_any_element()
    }
}
