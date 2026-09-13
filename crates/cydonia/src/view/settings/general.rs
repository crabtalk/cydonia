//! The general section: what this copy of cydonia is, whether a newer one is
//! out, and where the people who use it are.

use crate::{
    assets,
    model::update::{self, Status, Updater},
    view::settings::SettingsWindow,
};
use bezel::{
    gpui::{AnyElement, Context, Entity, SharedString, div, img, prelude::*, px},
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{ButtonStyle, Buttons, Content, Controls, Scaffolding},
};

/// What this build is, read at compile time from `Cargo.toml` — the same
/// string the bundle carries, since the Makefile stamps `CFBundleVersion` out
/// of that file too. Nothing here can drift from what was shipped.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The commit it was built from — see `build.rs`, which is the only place that
/// can know: the app that ships has no repository to ask.
const COMMIT: &str = env!("CYDONIA_COMMIT");

/// Where the people using this talk to each other.
const COMMUNITY: &str = "https://discord.gg/yGZDYnwbx6";

/// The mark over the rows. An About panel's measure — big enough to be the
/// picture of the app, small enough that the two lines under it are still what
/// the section is.
const MARK: f32 = 72.;

impl SettingsWindow {
    pub(super) fn general_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(super::GROUP_GAP))
            .children(assets::mark().map(|path| {
                div()
                    .flex()
                    .justify_center()
                    .child(img(path).size(px(MARK)))
            }))
            .child(
                theme
                    .group_box()
                    .child(
                        theme
                            .card_row(true)
                            .child(div().flex_1().min_w_0().child(theme.row_title("Version")))
                            .child(theme.badge(VERSION)),
                    )
                    .child(
                        theme
                            .card_row(false)
                            .child(div().flex_1().min_w_0().child(theme.row_title("Commit")))
                            .child(match commit_url() {
                                Some(url) => div()
                                    .id("commit")
                                    .cursor_pointer()
                                    .hover(|el| el.text_color(theme.accent))
                                    .child(theme.badge(COMMIT))
                                    .on_click(move |_, _, cx| cx.open_url(&url))
                                    .into_any_element(),
                                None => theme.badge(COMMIT).into_any_element(),
                            }),
                    ),
            )
            .children(self.updates(cx))
            .child(
                div().flex().justify_center().child(
                    div()
                        .id("community")
                        .cursor_pointer()
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .hover(|el| el.text_color(theme.accent))
                        .child("Community")
                        .on_click(|_, _, cx| cx.open_url(COMMUNITY)),
                ),
            )
            .into_any_element()
    }

    /// Releases: whether the app looks for one itself, and where the looking has
    /// got to. Absent whole on a build no release could replace — see
    /// [`crate::model::update`], which is also what decides whether the menu bar
    /// carries a check.
    fn updates(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let updater = update::of(cx)?;
        let theme = Theme::of(cx).clone();
        let auto = self.workspace.read(cx).settings.auto_update;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(super::LABEL_GAP))
                .child(theme.field_label("Updates"))
                .child(
                    theme
                        .group_box()
                        .child(
                            theme
                                .card_row(true)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(theme.row_title("Automatic updates"))
                                        .child(
                                            div()
                                                .mt(px(4.))
                                                .truncate()
                                                .text_style(TextStyle::Subheadline)
                                                .text_color(theme.text_muted)
                                                .child(
                                                    "Look for a release, and fetch it ready to \
                                                     restart into.",
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("auto-update")
                                        .cursor_pointer()
                                        .child(theme.toggle(auto))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.workspace.update(cx, |workspace, cx| {
                                                workspace.set_auto_update(!auto, cx)
                                            });
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(self.release_row(&updater, cx)),
                )
                .into_any_element(),
        )
    }

    /// What the updater is doing, and the one thing to do about it. The restart
    /// is the only control here that is prominent: it is the only one that acts
    /// on the app rather than on what it knows.
    fn release_row(&self, updater: &Entity<Updater>, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        let status = updater.read(cx).status().clone();
        let (line, detail): (SharedString, Option<SharedString>) = match &status {
            Status::Idle => ("Releases".into(), Some("Nothing asked for yet.".into())),
            Status::Checking => ("Looking for a release…".into(), None),
            Status::Current => ("cydonia is up to date".into(), None),
            Status::Downloading(version) => (format!("Fetching cydonia {version}…").into(), None),
            Status::Ready { version, .. } => (
                format!("cydonia {version} is ready").into(),
                Some("It goes in as the app restarts.".into()),
            ),
            Status::Failed(err) => ("No release could be fetched".into(), Some(err.clone())),
        };
        let working = matches!(status, Status::Checking | Status::Downloading(_));
        let ready = matches!(status, Status::Ready { .. });
        let updater = updater.clone();
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(line))
                    .children(detail.map(|copy| {
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(copy)
                    })),
            )
            .child(if working {
                div()
                    .flex_none()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_faint)
                    .child("working…")
                    .into_any_element()
            } else if ready {
                theme
                    .button("Restart to Update", ButtonStyle::Prominent, None)
                    .id("restart")
                    .flex_none()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        updater.update(cx, |updater, cx| updater.restart(cx));
                    }))
                    .into_any_element()
            } else {
                theme
                    .button(
                        "Check Now",
                        ButtonStyle::Ghost,
                        Some(Fade::new(painter, "check-now")),
                    )
                    .id("check-now")
                    .flex_none()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        updater.update(cx, |updater, cx| updater.check(true, cx));
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }
}

/// The commit on the page that hosts it, where there is one to open. A build
/// that names no commit links nowhere; one built over an edited tree still
/// links at the commit underneath, which is the last thing anybody else can
/// fetch.
fn commit_url() -> Option<String> {
    let sha = COMMIT.split('-').next()?;
    (sha != "unknown").then(|| format!("{}/commit/{sha}", env!("CARGO_PKG_REPOSITORY")))
}
