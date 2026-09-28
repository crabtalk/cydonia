//! The browser section: the in-app browser's preferences, written under
//! `[browser]`.

use crate::{
    model::settings::SEARCH_ENGINES,
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
                    .child(self.engine_row(cx))
                    .child(self.home_row(cx)),
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
            .card_row(true)
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
                        SEARCH_ENGINES
                            .iter()
                            .enumerate()
                            .map(|(ix, (name, template))| {
                                let selected = *template == search;
                                div()
                                    .id(("search-engine", ix))
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
                                    .child(*name)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.workspace.update(cx, |workspace, cx| {
                                            workspace.set_browser_search((*template).to_owned(), cx)
                                        });
                                        cx.notify();
                                    }))
                            }),
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
