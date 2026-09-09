//! The general section: what this copy of cydonia is, and where the people
//! who use it are.

use crate::{assets, view::settings::SettingsWindow};
use bezel::{
    gpui::{AnyElement, Context, div, img, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Content, Scaffolding},
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
}

/// The commit on the page that hosts it, where there is one to open. A build
/// that names no commit links nowhere; one built over an edited tree still
/// links at the commit underneath, which is the last thing anybody else can
/// fetch.
fn commit_url() -> Option<String> {
    let sha = COMMIT.split('-').next()?;
    (sha != "unknown").then(|| format!("{}/commit/{sha}", env!("CARGO_PKG_REPOSITORY")))
}
