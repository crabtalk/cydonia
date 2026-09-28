//! The browser section: the in-app browser's preferences, written under
//! `[browser]`.

use crate::{
    model::settings::{Links, SEARCH_ENGINES},
    view::{
        component::browser,
        settings::{self, SettingsWindow, Switch},
    },
};
use bezel::{
    gpui::{AnyElement, Context, PathPromptOptions, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{ButtonStyle, Buttons, Scaffolding},
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
            .child(
                theme
                    .group_box()
                    .child(self.agents_read_row(cx))
                    .child(self.agents_act_row(cx))
                    .child(self.agents_blocked_row(cx)),
            )
            .child(
                theme
                    .group_box()
                    .child(self.keep_signed_in_row(cx))
                    .child(self.clear_row(cx)),
            )
            .child(theme.group_box().child(self.downloads_row(cx)))
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

    fn agents_read_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.browser.agents_read;
        self.switch_row(
            Switch::new(
                "browser-agents-read",
                "Agents may read pages",
                "Offers your agents the tools that open, read and scroll browser tabs.",
                on,
            )
            .first(true)
            .truncate(),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_browser_agents_read(!on, cx)
                });
            },
        )
    }

    /// Read only while [`Self::agents_read_row`] is on.
    fn agents_act_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.browser.agents_act;
        self.switch_row(
            Switch::new(
                "browser-agents-act",
                "Agents may click and type",
                "Needs Read. Offers the tools that click and fill in fields on a page.",
                on,
            )
            .truncate(),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_browser_agents_act(!on, cx)
                });
            },
        )
    }

    fn agents_blocked_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let blocked = self
            .workspace
            .read(cx)
            .settings
            .browser
            .agents_blocked
            .join(", ");
        let current = blocked.clone();
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Sites agents may not use"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(match blocked.is_empty() {
                                true => "None.".to_owned(),
                                false => blocked,
                            }),
                    ),
            )
            .child(
                theme
                    .ghost("browser-blocked")
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
                        this.edit_field(
                            settings::Field::BrowserBlocked,
                            current.clone(),
                            window,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    /// Take the typed hosts: split on commas and whitespace, a URL cut to its
    /// host.
    pub(super) fn save_browser_blocked(&mut self, typed: &str, cx: &mut Context<Self>) {
        let mut hosts: Vec<String> = typed
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                url::Url::parse(entry)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .unwrap_or_else(|| entry.to_owned())
                    .to_ascii_lowercase()
            })
            .collect();
        hosts.dedup();
        self.workspace.update(cx, |workspace, cx| {
            workspace.set_browser_agents_blocked(hosts, cx)
        });
    }

    fn keep_signed_in_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.browser.keep_signed_in;
        self.switch_row(
            Switch::new(
                "browser-keep",
                "Keep signed in",
                "Off opens new tabs with nothing saved, forgotten when they close.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_browser_keep_signed_in(!on, cx)
                });
            },
        )
    }

    /// Clears through the pages that are open: a store is reached through a
    /// built page.
    fn clear_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let live = browser::any_page(cx);
        let note = match (self.browser_cleared, live) {
            (true, _) => "Cleared.",
            (false, true) => "Cookies, site storage and cache.",
            (false, false) => "Open a browser tab to clear what it stores.",
        };
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Clear browsing data"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(note),
                    ),
            )
            .child(
                theme
                    .button("Clear", ButtonStyle::Ghost, None)
                    .id("browser-clear")
                    .flex_none()
                    .when(!live, |button| button.opacity(0.5))
                    .when(live, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| {
                            this.browser_cleared = browser::clear_data(cx);
                            cx.notify();
                        }))
                    }),
            )
            .into_any_element()
    }

    fn downloads_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let folder = self.workspace.read(cx).settings.browser.downloads.clone();
        let chosen = folder.is_some();
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Download folder"))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(match folder {
                                Some(folder) => folder.display().to_string(),
                                None => "Your Downloads folder.".to_owned(),
                            }),
                    ),
            )
            .when(chosen, |row| {
                row.child(
                    theme
                        .button("Reset", ButtonStyle::Ghost, None)
                        .id("browser-downloads-reset")
                        .flex_none()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_browser_downloads(None, cx)
                            });
                        })),
                )
            })
            .child(
                theme
                    .button("Choose…", ButtonStyle::Ghost, None)
                    .id("browser-downloads")
                    .flex_none()
                    .on_click(cx.listener(|this, _, _, cx| this.choose_downloads(cx))),
            )
            .into_any_element()
    }

    /// Ask the system for a folder. Applies to pages built after it.
    fn choose_downloads(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(folder) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_browser_downloads(Some(folder), cx)
                });
                cx.notify();
            });
        })
        .detach();
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
