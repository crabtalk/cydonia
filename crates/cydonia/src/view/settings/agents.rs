//! The agents section: what the registry publishes, what is on this machine,
//! and the install or removal in flight.

use crate::{
    agent::{self, Listing},
    model::settings,
    view::{menubar, settings::SettingsWindow},
};
use bezel::{
    gpui::{AnyElement, Context, Focusable as _, SharedString, div, prelude::*, px},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons, loaders, popover,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Content, Scaffolding, Status},
    },
};
use cacp_agents::{Distribution, registry};
use tokio::sync::mpsc;

/// Where the version goes when it is pressed: the page the agent is published
/// on, at the version the row is about.
///
/// A package is its registry page pinned to that release; an agent that ships
/// no package — a binary, which is every proprietary one — is the page its
/// publisher documents it on, because that is the whole of what it publishes.
/// Nothing at all for one that names neither, and the version is then a tag
/// rather than a link.
/// How wide the dialog's label column is: enough for the longest of the three
/// at Subheadline, so the values start on one line down the table.
const LABEL_WIDTH: f32 = 72.;

/// How wide the dialog stands. Wider than the app's other one — see
/// [`super::performance::DIALOG_WIDTH`] — because this one carries a table and
/// that one carries a number.
const DIALOG_WIDTH: f32 = 400.;

/// A URL as an address: the scheme is noise, and so is a trailing slash.
fn trimmed(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

/// What a fetch is called, out of what it is fetched by: the file at the end of
/// a download, and the whole of a package spec, which is already a name.
fn named(fetching: &str) -> String {
    match fetching.starts_with("http") {
        true => fetching
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| trimmed(fetching)),
        false => fetching.to_owned(),
    }
}

fn source_url(agent: &registry::Agent, version: &str) -> Option<String> {
    if let Distribution::Npm { package, .. } = &agent.distribution {
        let name = cacp_agents::package_name(package);
        return Some(format!("https://www.npmjs.com/package/{name}/v/{version}"));
    }
    agent.website.clone().or_else(|| agent.repository.clone())
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

    /// Install, or ask first where this agent's source has not been agreed to.
    ///
    /// The question is about the source and not the release, so an update to
    /// something already agreed to goes straight through — see
    /// [`crate::model::settings::Settings::trusted_agents`].
    fn install(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(agent) = self
            .listings
            .as_ref()
            .and_then(|l| l.get(ix))
            .map(|listing| listing.agent.clone())
        else {
            return;
        };
        let agreed = self
            .workspace
            .read(cx)
            .settings
            .trusted_agents
            .get(&agent.id)
            .is_some_and(|mark| *mark == agent::source_mark(&agent));
        match agreed {
            true => self.fetch(ix, cx),
            false => {
                self.trusting = Some(ix);
                cx.notify();
            }
        }
    }

    /// Agree to this agent's source, write that down, and go on with the
    /// install the dialog interrupted.
    fn agreed(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.trusting = None;
        let Some(agent) = self
            .listings
            .as_ref()
            .and_then(|l| l.get(ix))
            .map(|listing| listing.agent.clone())
        else {
            return;
        };
        if let Err(err) = settings::trust_agent(&agent.id, &agent::source_mark(&agent)) {
            self.error = Some(format!("{err:#}").into());
            cx.notify();
            return;
        }
        self.workspace
            .update(cx, |workspace, cx| workspace.reload_settings(cx));
        self.fetch(ix, cx);
    }

    /// Put the agent on disk and name it in `settings.toml`, then read that
    /// file back so the composer's list and the file cannot disagree.
    fn fetch(&mut self, ix: usize, cx: &mut Context<Self>) {
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
        self.output.remove(&id);
        self.error = None;
        cx.notify();
        // The installer prints from the background executor, where there is no
        // `cx` to notify with. The lines come back over a channel and are
        // drained here instead — the same shape the agent's own stderr takes,
        // see [`crate::model::session::pump`].
        let (tx, mut lines) = mpsc::unbounded_channel();
        let reading = id.clone();
        cx.spawn(async move |this, cx| {
            let installing = cx.background_executor().spawn(async move {
                agent::install(&agent, |line| {
                    let _ = tx.send(line.to_owned());
                })
            });
            // Ends when the install drops the sender, which is the install
            // being over — so the result is waited for after, not raced with.
            while let Some(line) = lines.recv().await {
                let _ = this.update(cx, |this, cx| this.printed(&reading, line, cx));
            }
            let done = installing.await;
            let _ = this.update(cx, |this, cx| this.settled(id, done, cx));
        })
        .detach();
    }

    /// One line of the installer's account of itself.
    fn printed(&mut self, id: &str, line: String, cx: &mut Context<Self>) {
        agent::record(self.output.entry(id.to_owned()).or_default(), line);
        cx.notify();
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
                // Nothing left to explain, so the account of it goes with the
                // spinner rather than sitting under a row that is now fine.
                self.output.remove(&id);
                self.workspace
                    .update(cx, |workspace, cx| workspace.reload_settings(cx));
                // File lists the agents by name, and AppKit holds the tree it
                // was handed until it is handed another.
                menubar::refresh(cx);
                self.load(cx);
            }
            // The output stays: the message names what failed and the lines
            // under it are the only place that says why.
            Err(err) => self.error = Some(format!("{err:#}").into()),
        }
        cx.notify();
    }

    /// What an install is agreed to before it runs.
    ///
    /// Shown once per agent, on the source rather than on the release: a new
    /// version of something already agreed to is the same decision, and asking
    /// again on every update is how a dialog becomes a thing people dismiss
    /// without reading.
    ///
    /// It states what is about to be fetched and what will be able to run, and
    /// makes no claim about whether the agent is safe: the registry lists
    /// clients, it does not vet them, and cydonia has read none of their code.
    pub(super) fn trust_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ix = self.trusting?;
        let listing = self.listings.as_ref()?.get(ix)?;
        let theme = Theme::of(cx).clone();
        let agent = &listing.agent;
        let publisher = agent.website.clone().or_else(|| agent.repository.clone());
        // What lands on the machine, spelled the way the installer will fetch
        // it. The label changes with the kind, because "package" and
        // "download" are not the same promise.
        let (kind, fetching, integrity) = match &agent.distribution {
            Distribution::Npm { package, .. } => (
                "Package",
                SharedString::from(package.clone()),
                Some(("Install scripts are not run", false)),
            ),
            Distribution::Binary(binary) => (
                "Download",
                SharedString::from(binary.archive.clone()),
                Some(match binary.sha256.is_some() {
                    true => ("Checked against the publisher's checksum", false),
                    // The one case worth colouring, and the reason this is per
                    // agent rather than one line over all of them.
                    false => ("No checksum published — unverified", true),
                }),
            ),
            Distribution::Unsupported { .. } => {
                ("Source", SharedString::from(agent.id.clone()), None)
            }
        };
        // A label column and a value column: the values line up, and a long one
        // is cut rather than wrapped into a paragraph of URL.
        let row = |label: &'static str, value: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .flex_none()
                        .w(px(LABEL_WIDTH))
                        .text_style(TextStyle::Subheadline)
                        .text_color(theme.text_faint)
                        .child(label),
                )
                .child(value)
        };
        // Cut to what tells the two apart — a file name, a host and path — with
        // the whole of it a hover away. A URL set in full wraps to three lines
        // and becomes the loudest thing in the dialog.
        let address = |text: String, url: String| {
            div()
                .id(SharedString::from(format!("trust-{text}")))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .cursor_pointer()
                .text_style(TextStyle::Subheadline)
                .text_color(theme.text)
                .hover(|el| el.text_color(theme.accent))
                .tooltip({
                    let whole = SharedString::from(url.clone());
                    move |window, cx| Tooltip::text(whole.clone(), window, cx)
                })
                .child(text)
                .on_click(move |_, _, cx| cx.open_url(&url))
                .into_any_element()
        };
        Some(
            div()
                .id("trust-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.trusting = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("trust-dialog")
                        .w(px(DIALOG_WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .p(px(20.))
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .on_click(|_, _, cx| cx.stop_propagation())
                        // The row it came from, restated: the same mark, the
                        // same name and the same tag, so the dialog reads as
                        // that row opened rather than as a page of its own.
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.))
                                .children(listing.icon.clone().map(|icon| theme.row_icon(icon)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(theme.row_title(agent.name.clone())),
                                )
                                .child(theme.badge(format!("v{}", agent.version))),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child("Listed in the ACP registry, which cydonia does not review."),
                        )
                        .child(
                            theme
                                .group_box()
                                .p(px(12.))
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .children(publisher.map(|url| {
                                    row("Publisher", address(trimmed(&url), url))
                                }))
                                .child(row(kind, address(named(&fetching), fetching.to_string())))
                                .children(integrity.map(|(copy, loud)| {
                                    row(
                                        "Integrity",
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_style(TextStyle::Subheadline)
                                            .text_color(match loud {
                                                true => theme.warning,
                                                false => theme.text_muted,
                                            })
                                            .child(copy)
                                            .into_any_element(),
                                    )
                                })),
                        )
                        .child(
                            div()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child(
                                    "It runs when you open a session on it, and reads and writes the project you point it at.",
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    theme
                                        .button("Cancel", ButtonStyle::Ghost, None)
                                        .id("trust-cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.trusting = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    theme
                                        .button("Install", ButtonStyle::Prominent, None)
                                        .id("trust-install")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.agreed(ix, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// One agent: its mark and name, what it is, and the one thing you can do
    /// about it.
    fn agent_row(
        &self,
        ix: usize,
        listing: &Listing,
        first: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let busy = self.busy.contains(&listing.agent.id);
        let status = busy
            .then(|| {
                self.output
                    .get(&listing.agent.id)
                    .and_then(|held| held.last())
            })
            .flatten()
            .map(|line| SharedString::from(line.clone()));
        let installed = listing.installed.clone();
        // The version this row is about: what is on disk while there is
        // something on disk, and what the registry pins otherwise. The tag and
        // the link both read it, so they cannot point at different releases.
        let version = installed
            .clone()
            .unwrap_or_else(|| listing.agent.version.clone());
        // What the registry pins now, where that is not what is on disk. A
        // string comparison and not an ordering: the registry is what says
        // which release an agent is at, and a machine holding a version it no
        // longer names is out of step whichever way the numbers run.
        let update = installed
            .as_ref()
            .filter(|held| **held != listing.agent.version)
            .map(|_| listing.agent.version.clone());
        let outdated = update.is_some();
        let source = source_url(&listing.agent, &version);
        // The row is the group the removal reads: at rest an outdated row
        // shows one glyph, and the pointer brings the other back.
        let group = SharedString::from(format!("agent-row-{ix}"));
        theme
            .card_row(first)
            // The row fills its box. Inside a `flex_col` group that is what
            // stretch does anyway, but a virtual list measures each row on its
            // own and would hand back a row as wide as its name.
            .w_full()
            .group(group.clone())
            .child(
                div()
                    .flex_none()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(
                        listing.icon.clone().map(|icon| {
                            icons::icon(icon).size(px(16.)).text_color(theme.text_muted)
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(listing.agent.name.clone()))
                    // The installer's line, and nothing at rest: where the
                    // agent is published is the version's to say — see
                    // [`source_url`] — so a resting row is its name and the one
                    // thing you can do about it.
                    .children(status.map(|status| {
                        div()
                            .mt(px(2.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_style(TextStyle::Caption)
                            .text_color(theme.text_faint)
                            .child(status)
                    })),
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
                    .child(match source {
                        // The tag is the link: one press from the row to the
                        // release it names, and the row keeps one link rather
                        // than a line of them.
                        Some(url) => theme
                            .badge(format!("v{version}"))
                            .id(("version", ix))
                            .cursor_pointer()
                            .hover(|el| el.text_color(theme.accent))
                            .tooltip({
                                let address = SharedString::from(url.clone());
                                move |window, cx| Tooltip::text(address.clone(), window, cx)
                            })
                            .on_click(move |_, _, cx| cx.open_url(&url))
                            .into_any_element(),
                        None => theme.badge(format!("v{version}")).into_any_element(),
                    }),
            )
            .child(
                div()
                    .flex_none()
                    // Wide enough for the two glyphs an outdated row carries,
                    // on every row: a column that grew only where there is an
                    // update would step in and out down the list.
                    .w(px(64.))
                    .flex()
                    .gap(px(4.))
                    .items_center()
                    // A button fills the slot and ends where the card does, so
                    // it hangs off the right. The spinner is a few pixels wide
                    // and takes the middle instead — pushed to that same edge
                    // it reads as having fallen off the row.
                    .when(busy, |el| el.justify_center())
                    .when(!busy, |el| el.justify_end())
                    .child(match (busy, installed, listing.agent.installable()) {
                        // A spinner, not a label: an install runs for as long
                        // as `npm` does, and a word that never moves reads as a
                        // row that has hung.
                        (true, _, _) => div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .child(loaders::mini_gradient_spinner(
                                SharedString::from(format!("installing-{ix}")),
                                2.5,
                                painter,
                                cx,
                            ))
                            .into_any_element(),
                        // One glyph at rest. An outdated row shows the update,
                        // and the removal waits for the pointer: a row that is
                        // behind is one to catch up, not one to take off, and
                        // two glyphs side by side read as a choice about which.
                        //
                        // Revealed rather than dropped — removing an agent the
                        // registry has moved on from is a reasonable thing to
                        // want, and hiding it outright would mean fetching an
                        // update first to reach it.
                        (false, Some(_), _) => div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.))
                            .children(update.map(|to| {
                                theme
                                    .icon_button(
                                        icons::arrows::RefreshCw,
                                        ButtonStyle::Ghost,
                                        Some(Fade::new(painter, format!("update-{ix}"))),
                                    )
                                    .id(("update", ix))
                                    .flex_none()
                                    .tooltip(move |window, cx| {
                                        Tooltip::text(format!("Update to v{to}"), window, cx)
                                    })
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.install(ix, cx)),
                                    )
                            }))
                            .child(
                                theme
                                    .icon_button(
                                        icons::files::Trash,
                                        ButtonStyle::Ghost,
                                        Some(Fade::new(painter, format!("remove-{ix}"))),
                                    )
                                    .id(("remove", ix))
                                    .flex_none()
                                    .when(outdated, |el| {
                                        el.invisible().group_hover(group.clone(), |el| el.visible())
                                    })
                                    .tooltip(|window, cx| Tooltip::text("Remove", window, cx))
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.remove(ix, cx)),
                                    ),
                            )
                            .into_any_element(),
                        (false, None, true) => theme
                            .icon_button(
                                icons::files::Download,
                                ButtonStyle::Ghost,
                                Some(Fade::new(painter, format!("install-{ix}"))),
                            )
                            .id(("install", ix))
                            .flex_none()
                            .tooltip(|window, cx| Tooltip::text("Install", window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| this.install(ix, cx)))
                            .into_any_element(),
                        // Published, but with no build this machine can run.
                        (false, None, false) => div()
                            .flex_none()
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text_faint)
                            .child("unavailable")
                            .into_any_element(),
                    }),
            )
            .into_any_element()
    }

    /// The agents section: an ACP client manager and nothing else — what is on
    /// this machine, and what the registry supports.
    ///
    /// The gate is not here. It is the `sessions` switch, which lives with the
    /// other surfaces: what may run is a question about the app, and this is
    /// the room where clients are put on and taken off.
    pub(super) fn agents_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        // The height runs from the page column to the list at the bottom of
        // this, and every box between them has to pass it on: a `gpui::list`
        // given no height builds no rows, and an empty catalogue is what that
        // looks like.
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::GROUP_GAP))
            .children(
                self.error
                    .clone()
                    .map(|err| theme.error_strip(err).flex_none()),
            )
            .children(self.failed_output(cx))
            .child(self.catalogue(cx))
            .into_any_element()
    }

    /// What the installer said on the way down, under the message naming what
    /// failed.
    ///
    /// Only after a failure — [`Self::settled`] drops the lines of an install
    /// that worked. The row's own status carries the last of them while it
    /// runs, which is as much as anyone wants of a working install; this is the
    /// one case where the rest of it is the answer.
    fn failed_output(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        self.error.as_ref()?;
        let held: Vec<String> = self.output.values().flatten().cloned().collect();
        if held.is_empty() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .p(px(10.))
                .rounded(px(Theme::surface_radius()))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .text_style(TextStyle::Caption)
                .text_color(theme.text_muted)
                .children(held.into_iter().map(|line| div().child(line)))
                .into_any_element(),
        )
    }

    /// What is on this machine, and what could be — once the registry has
    /// answered.
    ///
    /// The installed box carries no label. It is the first thing under the
    /// title and it holds the clients this app has: a heading over it would
    /// name what is already plain, and with nothing installed there is no box
    /// at all rather than a labelled empty one.
    fn catalogue(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(listings) = self.listings.as_ref() else {
            return theme
                .group_box()
                .child(self.note("Reading the agent catalogue…", true, cx))
                .into_any_element();
        };
        if listings.is_empty() {
            return theme
                .empty_state(
                    icons::development::Bot,
                    "No catalogue",
                    "The agent registry could not be reached.",
                )
                .into_any_element();
        }
        let query = self.search.read(cx).content();
        // The query belongs to the list below and to nothing else: what is on
        // this machine stays on screen while a name is being looked for, and
        // an agent leaves that list by being installed rather than by not
        // matching — a row in both boxes is one row twice.
        let installed: Vec<usize> = self
            .ranked(listings, "")
            .into_iter()
            .filter(|&ix| listings[ix].installed.is_some())
            .collect();
        let supported: Vec<usize> = self
            .ranked(listings, query.trim())
            .into_iter()
            .filter(|&ix| listings[ix].installed.is_none())
            .collect();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::GROUP_GAP))
            // What is on this machine is a handful and stands in flow. The
            // catalogue below it is what scrolls.
            .children((!installed.is_empty()).then(|| {
                theme
                    .group_box()
                    .flex_none()
                    .children(self.agent_rows(installed, true, cx))
            }))
            .child(self.supported(supported, cx))
            .into_any_element()
    }

    /// What can still be put on this machine, headed by the query that narrows
    /// it: what is searched and what the search leaves are one box, because
    /// the second is the answer to the first.
    fn supported(&self, rows: Vec<usize>, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let empty = rows.is_empty();
        let listings = self.listings.as_deref().unwrap_or_default();
        // Keyed by agent and not by row: a query that narrows the list splices
        // it, and the rows that survive keep the place they were scrolled to.
        let filling = self.agents_list.state.item_count() == 0;
        self.agents_list.sync(
            rows.iter()
                .filter_map(|&ix| listings.get(ix))
                .map(|listing| listing.agent.id.clone())
                .collect(),
        );
        // A list filled from empty is read from its first row. Said rather
        // than left to the default: `ListState` carries an anchor of item 0
        // from the moment it is made, and a splice that inserts in front of
        // the anchor carries it along — so the rows arrive with the list
        // scrolled past all of them.
        if filling {
            self.agents_list.scroll_to(0);
        }
        let view = cx.entity();
        let held = rows.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::LABEL_GAP))
            .child(theme.field_label("Supported").flex_none())
            .child(
                theme
                    .group_box()
                    .flex_1()
                    .min_h_0()
                    .child(self.search_row(cx))
                    .children(empty.then(|| self.note("No matches.", false, cx)))
                    // Only the rows the viewport reaches are built, so the
                    // layout engine is handed a screenful rather than the
                    // registry.
                    .children((!empty).then(|| {
                        div().flex_1().min_h_0().child(self.agents_list.render(
                            move |at, _, cx| {
                                let Some(&ix) = held.get(at) else {
                                    return div().into_any_element();
                                };
                                view.update(cx, |this, cx| {
                                    let held = this.listings.as_deref().unwrap_or_default();
                                    let Some(listing) = held.get(ix) else {
                                        return div().into_any_element();
                                    };
                                    // Never the first: the query line heads the
                                    // box, so every row keeps its hairline.
                                    this.agent_row(ix, listing, false, cx)
                                })
                            },
                            |_, _, _| {},
                        ))
                    })),
            )
            .into_any_element()
    }

    /// The query line: frameless, since the box it heads is the frame.
    ///
    /// Nothing focuses it — the section opens on a list to read, not on a
    /// field to type in — and a press anywhere else gives the focus back, so
    /// the caret is only ever blinking where it was put.
    fn search_row(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(true)
            .flex_none()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                if this.search.read(cx).focus_handle(cx).is_focused(window) {
                    window.blur(cx);
                }
            }))
            .child(
                icons::icon(icons::text::Search)
                    .size(px(14.))
                    .flex_none()
                    .text_color(theme.text_faint),
            )
            .child(div().flex_1().min_w_0().child(self.search.clone()))
            .into_any_element()
    }

    /// Which listings the query keeps, in the order it ranks them: the
    /// registry's own order is alphabetical, and a search's is how well each
    /// name answers what was typed.
    fn ranked(&self, listings: &[Listing], query: &str) -> Vec<usize> {
        if !query.is_empty() {
            let names: Vec<&str> = listings
                .iter()
                .map(|listing| listing.agent.name.as_str())
                .collect();
            return popover::filter_indices(query, &names);
        }
        let mut order: Vec<usize> = (0..listings.len()).collect();
        // What cannot be installed here sinks: it is on the list to say the
        // registry knows it, not to be reached for.
        order.sort_by_key(|&ix| {
            (
                !listings[ix].agent.installable(),
                listings[ix].agent.name.to_lowercase(),
            )
        });
        order
    }

    /// The rows of one box. `heads` is whether the first of them opens the box
    /// — under the search line it does not, and the hairline stays.
    fn agent_rows(&self, rows: Vec<usize>, heads: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let listings = self.listings.as_deref().unwrap_or_default();
        rows.into_iter()
            .enumerate()
            .filter_map(|(nth, ix)| {
                Some(self.agent_row(ix, listings.get(ix)?, heads && nth == 0, cx))
            })
            .collect()
    }

    /// A quiet line where a row would be: still reading, or nothing to read.
    fn note(&self, copy: &'static str, first: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(first)
            .child(
                div()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(copy),
            )
            .into_any_element()
    }
}
