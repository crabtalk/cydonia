//! The browser section: the in-app browser's preferences, written under
//! `[browser]`.

use crate::{
    model::settings::{Links, SEARCH_ENGINES},
    view::settings::{self, SettingsWindow},
};
use bezel::{
    gpui::{AnyElement, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{Buttons, Scaffolding},
};

impl SettingsWindow {
    pub(super) fn browser_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(
                theme
                    .group_box()
                    .child(self.links_row(cx))
                    .child(self.engine_row(cx))
                    .child(self.home_row(cx)),
            )
            .into_any_element()
    }

    fn links_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let links = self.workspace.read(cx).settings.browser.links;
        const CHOICES: [(Links, &str); 2] = [
            (Links::Panel, "Browser tab"),
            (Links::System, "System browser"),
        ];
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Open web links in"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child("Links clicked in articles and sessions."),
                    ),
            )
            .child(
                self.segments(
                    "browser-links",
                    CHOICES
                        .iter()
                        .map(|(choice, label)| (*label, *choice == links))
                        .collect(),
                    cx,
                    |this, ix, cx| {
                        let links = CHOICES[ix].0;
                        this.workspace
                            .update(cx, |workspace, cx| workspace.set_browser_links(links, cx));
                    },
                ),
            )
            .into_any_element()
    }

    /// A segmented control: `(label, selected)` per segment, `pick` called
    /// with the index pressed.
    fn segments(
        &self,
        id: &'static str,
        segments: Vec<(&'static str, bool)>,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, usize, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex_none()
            .flex()
            .flex_row()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(Theme::button_radius()))
            .border_1()
            .border_color(theme.border)
            .children(
                segments
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (label, selected))| {
                        let pick = pick.clone();
                        div()
                            .id((id, ix))
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .text_style(TextStyle::Callout)
                            .cursor_pointer()
                            .when(selected, |el| {
                                el.bg(theme.element_active).text_color(theme.text)
                            })
                            .when(!selected, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.element_hover))
                            })
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                pick(this, ix, cx);
                                cx.notify();
                            }))
                    }),
            )
            .into_any_element()
    }

    /// The engines as segments. A template set by hand in `settings.toml`
    /// lights none, and the line under the title shows it.
    fn engine_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let search = self.workspace.read(cx).settings.browser.search.clone();
        let named = SEARCH_ENGINES
            .iter()
            .find(|(_, template)| *template == search)
            .map(|(name, _)| format!("Searches go to {name}."));
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Search engine"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(named.unwrap_or_else(|| format!("Searches go to {search}"))),
                    ),
            )
            .child(
                self.segments(
                    "search-engine",
                    SEARCH_ENGINES
                        .iter()
                        .map(|(name, template)| (*name, *template == search))
                        .collect(),
                    cx,
                    |this, ix, cx| {
                        let template = SEARCH_ENGINES[ix].1.to_owned();
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.set_browser_search(template, cx)
                        });
                    },
                ),
            )
            .into_any_element()
    }

    fn home_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let home = self.workspace.read(cx).settings.browser.home.clone();
        let current = home.clone();
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Home page"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(home),
                    ),
            )
            .child(
                theme
                    .ghost("browser-home")
                    .flex_none()
                    .px(px(10.))
                    .py(px(3.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.input_bg)
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text)
                    .child("Change…")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_field(settings::Field::BrowserHome, current.clone(), window, cx)
                    })),
            )
            .into_any_element()
    }

    /// Take the typed address. A bare host is taken over https; an empty
    /// field leaves the home page as it was.
    pub(super) fn save_browser_home(&mut self, typed: &str, cx: &mut Context<Self>) {
        let typed = typed.trim();
        if typed.is_empty() {
            return;
        }
        let home = match typed.contains("://") || typed.starts_with("about:") {
            true => typed.to_owned(),
            false => format!("https://{typed}"),
        };
        self.workspace
            .update(cx, |workspace, cx| workspace.set_browser_home(home, cx));
    }
}
