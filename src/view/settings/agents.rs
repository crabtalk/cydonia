//! The agents section: what the registry publishes, what is on this machine,
//! and the install or removal in flight.

use crate::{
    agent::{self, Listing},
    view::settings::SettingsWindow,
};
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px, svg},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Scaffolding, Status},
    },
};
use cacp_agents::{Distribution, registry};

/// An address that opens in the browser, shown as its own text.
fn link(
    id: (&'static str, usize),
    label: SharedString,
    url: String,
    theme: &Theme,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .hover(|el| el.text_color(theme.accent))
        .overflow_hidden()
        .whitespace_nowrap()
        .child(label)
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// The row's first link: what the agent is distributed as. A package is named,
/// at the version the row is about; an agent that ships no package — a binary,
/// which is every proprietary one — points at the page its publisher documents
/// it on, because that is the whole of what it publishes.
enum Source {
    Package { name: SharedString, url: String },
    Page { address: SharedString, url: String },
}

impl Source {
    /// Nothing at all for an agent that names neither.
    fn of(agent: &registry::Agent, version: &str) -> Option<Self> {
        if let Distribution::Npm { package, .. } = &agent.distribution {
            let name = cacp_agents::package_name(package);
            return Some(Self::Package {
                name: SharedString::from(name.to_owned()),
                url: format!("https://www.npmjs.com/package/{name}/v/{version}"),
            });
        }
        let url = agent.website.clone().or_else(|| agent.repository.clone())?;
        Some(Self::Page {
            address: SharedString::from(trimmed(&url)),
            url,
        })
    }

    /// What it reads as, and where it goes. Both variants are one link on the
    /// row: which of the two it came from is the labelling, not the shape.
    fn parts(self) -> (SharedString, String) {
        match self {
            Self::Package { name, url } => (name, url),
            Self::Page { address, url } => (address, url),
        }
    }
}

/// The repository, as its mark. The address is a line of noise in a row this
/// narrow, and the tooltip has it for whoever wants it.
fn repo_link(id: (&'static str, usize), url: String, theme: &Theme) -> impl IntoElement {
    let address = SharedString::from(trimmed(&url));
    div()
        .id(id)
        .group("agent-source")
        .flex_none()
        .cursor_pointer()
        .tooltip(move |window, cx| Tooltip::text(address.clone(), window, cx))
        .child(
            // On the glyph rather than on this box: an svg paints from its own
            // computed style, and inherits no colour from the row around it.
            icons::icon(icons::editing::GIT_BRANCH)
                .size(px(13.))
                .text_color(theme.text_faint)
                .group_hover("agent-source", |el| el.text_color(theme.accent)),
        )
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// A URL as an address rather than a link: the scheme is noise in a row.
fn trimmed(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

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
        // The version this row is about: what is on disk while there is
        // something on disk, and what the registry pins otherwise. The tag and
        // the link both read it, so they cannot point at different releases.
        let version = installed
            .clone()
            .unwrap_or_else(|| listing.agent.version.clone());
        let source = Source::of(&listing.agent, &version).map(Source::parts);
        // Where the source itself lives, when that is somewhere else: an agent
        // whose only page IS its repository says it once, as a link.
        let repository = listing
            .agent
            .repository
            .clone()
            .filter(|repo| source.as_ref().is_none_or(|(_, url)| url != repo));
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
                            .mt(px(2.))
                            .flex()
                            .gap(px(8.))
                            .overflow_hidden()
                            .text_style(TextStyle::Caption)
                            .text_color(theme.text_faint)
                            .children(
                                source.map(|(label, url)| link(("source", ix), label, url, &theme)),
                            )
                            .children(repository.map(|url| repo_link(("repo", ix), url, &theme))),
                    ),
            )
            // Two columns rather than two trailing children: widths off the
            // type ladder, so they hold their line down the card whatever a
            // version reads or which of the three states the control is in.
            .child(
                div()
                    .flex_none()
                    .w_16()
                    .flex()
                    .justify_end()
                    .child(theme.badge(format!("v{version}"))),
            )
            .child(
                div().flex_none().w_20().flex().justify_end().child(
                    match (busy, installed, listing.agent.installable()) {
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
                    },
                ),
            )
            .into_any_element()
    }

    /// The agents section: everything the registry publishes, installed first
    /// so what you already have is what you see. The gate itself is not here —
    /// it is the `sessions` switch, which lives with the other surfaces.
    pub(super) fn agents_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(super::GROUP_GAP))
            .children(self.error.clone().map(|err| theme.error_strip(err)))
            .children(self.sessions_off(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(super::LABEL_GAP))
                    .child(theme.field_label("Clients"))
                    .child(self.catalogue(cx)),
            )
            .into_any_element()
    }

    /// What the registry offers, once it has answered.
    fn catalogue(&self, cx: &Context<Self>) -> AnyElement {
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
                    icons::system::WIDGET,
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
        theme.group_box().children(rows).into_any_element()
    }
}
