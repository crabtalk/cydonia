//! What files and pictures open in: the default the `Open with` buttons use,
//! and which applications their `▾` menus leave out.

use crate::{
    model::settings::Opens,
    view::{
        component::file::external::{self, Application},
        settings::{self, SettingsWindow},
    },
};
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons, Layout, Scaffolding},
    },
};
use std::rc::Rc;

const DEFAULT: &str = "Default app";
const LISTING: &str = "Listing apps…";

fn glyph(opens: Opens) -> &'static [u8] {
    match opens {
        Opens::Files => icons::files::FileText,
        Opens::Pictures => icons::glyph::Image,
    }
}

fn blurb(opens: Opens) -> &'static str {
    match opens {
        Opens::Files => "The file view's Open button.",
        Opens::Pictures => "The button over a picture in a markdown preview.",
    }
}

fn title(opens: Opens) -> &'static str {
    match opens {
        Opens::Files => "Files",
        Opens::Pictures => "Pictures",
    }
}

/// The kind of file each list is asked for.
fn probe(opens: Opens) -> &'static str {
    match opens {
        Opens::Files => "txt",
        Opens::Pictures => "png",
    }
}

/// The applications found for each opener, `None` while listing, and which
/// opener's list is unfolded.
#[derive(Default)]
pub(super) struct Listed {
    files: Option<Rc<Vec<Application>>>,
    pictures: Option<Rc<Vec<Application>>>,
    open: Option<Opens>,
    /// Whether the unfolded opener's hidden applications are unfolded too.
    hidden_open: bool,
}

impl Listed {
    fn get(&self, opens: Opens) -> Option<Rc<Vec<Application>>> {
        match opens {
            Opens::Files => self.files.clone(),
            Opens::Pictures => self.pictures.clone(),
        }
    }

    fn set(&mut self, opens: Opens, apps: Vec<Application>) {
        let apps = Some(Rc::new(
            apps.into_iter().filter(Application::remembered).collect(),
        ));
        match opens {
            Opens::Files => self.files = apps,
            Opens::Pictures => self.pictures = apps,
        }
    }
}

impl SettingsWindow {
    /// List both openers' applications off the main thread.
    pub(super) fn list_apps(cx: &mut Context<Self>) {
        for opens in Opens::ALL {
            cx.spawn(async move |this, cx| {
                let listed = cx
                    .background_executor()
                    .spawn(
                        async move { external::listed_for(probe(opens), opens == Opens::Pictures) },
                    )
                    .await
                    .unwrap_or_default();
                let _ = this.update(cx, |this, cx| {
                    this.apps.set(opens, listed);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// `Open with`: a row per opener naming its default, unfolding into its
    /// applications — ✓ on the default, an eye for the `▾` menu.
    pub(super) fn apps_group(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let mut group = theme.group_box();
        for (ix, opens) in Opens::ALL.into_iter().enumerate() {
            group = group.child(self.opener_head(ix == 0, opens, &theme, cx));
            if self.apps.open == Some(opens) {
                group = group.child(self.opener_list(opens, &theme, cx));
            }
        }
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Open with"))
            .child(group)
            .into_any_element()
    }

    fn opener_head(
        &self,
        first: bool,
        opens: Opens,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let opener = external::opener(opens);
        let chosen = opener.app.clone();
        let current = self
            .apps
            .get(opens)
            .and_then(|apps| external::current(&apps, &opener).cloned());
        let (icon, name) = match (&current, &chosen) {
            (Some(app), _) => (app.icon(px(16.)), app.name().to_owned()),
            (None, Some(path)) => (
                default_icon(theme),
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            (None, None) => (default_icon(theme), LISTING.to_owned()),
        };
        let open = self.apps.open == Some(opens);
        theme
            .card_row(first)
            .id(("open-with", opens as usize))
            .cursor_pointer()
            .child(
                lead().child(
                    icons::icon(glyph(opens))
                        .size(px(16.))
                        .flex_none()
                        .text_color(theme.text_muted),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(title(opens)))
                    .child(
                        div()
                            .mt(px(4.))
                            .truncate()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(blurb(opens)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_style(TextStyle::Body)
                    .text_color(theme.text)
                    .child(icon)
                    .child(name),
            )
            .child(theme.disclosure(open))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apps.open = (!open).then_some(opens);
                this.apps.hidden_open = false;
                cx.notify();
            }))
            .into_any_element()
    }

    /// The unfolded rows: every choice the menu shows, the default among them,
    /// then the ones it leaves out, folded under one row.
    fn opener_list(&self, opens: Opens, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let list = div().flex().flex_col();
        let Some(apps) = self.apps.get(opens) else {
            return list
                .child(
                    theme.card_row(false).child(lead()).child(
                        div()
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(LISTING),
                    ),
                )
                .into_any_element();
        };
        let opener = external::opener(opens);
        let current = external::current(&apps, &opener).map(|app| app.path().to_owned());
        let choices = apps.iter().map(Some);
        let (hidden, shown): (Vec<_>, Vec<_>) =
            choices.partition(|app| app.is_some_and(|app| opener.hides(app.path())));
        let mut rows: Vec<AnyElement> = shown
            .into_iter()
            .enumerate()
            .map(|(ix, app)| {
                let chosen = current.as_deref() == app.map(Application::path);
                self.choice_row(opens, ix, app, chosen, false, theme, cx)
            })
            .collect();
        if !hidden.is_empty() {
            let open = self.apps.hidden_open;
            rows.push(
                theme
                    .card_row(false)
                    .id(("open-with-hidden", opens as usize))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_style(TextStyle::Body)
                            .text_color(theme.text_muted)
                            .child(format!("Hidden ({})", hidden.len())),
                    )
                    .child(theme.disclosure(open))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apps.hidden_open = !open;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if open {
                rows.extend(hidden.into_iter().enumerate().map(|(ix, app)| {
                    self.choice_row(opens, 500 + ix, app, false, true, theme, cx)
                }));
            }
        }
        list.children(rows).into_any_element()
    }

    /// One choice: ✓ on the default, a press makes it the default, and the
    /// eye, shown on hover, flips whether the menu shows it. The default has
    /// no eye.
    #[allow(clippy::too_many_arguments)]
    fn choice_row(
        &self,
        opens: Opens,
        ix: usize,
        app: Option<&Application>,
        chosen: bool,
        hidden: bool,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = ix + opens as usize * 1000;
        let path = app.map(|app| app.path().to_owned());
        let (icon, name) = match app {
            Some(app) => (app.icon(px(16.)), app.name().to_owned()),
            None => (default_icon(theme), DEFAULT.to_owned()),
        };
        let group: SharedString = format!("open-with-app-{id}").into();
        let eye = path.clone().filter(|_| !chosen).map(|path| {
            theme
                .icon_button(
                    match hidden {
                        true => icons::glyph::EyeOff,
                        false => icons::glyph::Eye,
                    },
                    ButtonStyle::Ghost,
                    None,
                )
                .id(("open-with-eye", id))
                .flex_none()
                .invisible()
                .group_hover(group.clone(), |el| el.visible())
                .tooltip(move |window, cx| {
                    let tip = match hidden {
                        true => "Show in the Open with menu",
                        false => "Hide from the Open with menu",
                    };
                    Tooltip::text(tip, window, cx)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    external::hide(opens, &path, !hidden);
                    cx.notify();
                }))
        });
        theme
            .card_row(false)
            .id(("open-with-app", id))
            .group(group)
            .cursor_pointer()
            .hover(|el| el.bg(theme.element_hover))
            .child(lead().children(chosen.then(|| {
                icons::icon(icons::glyph::Check)
                    .size(px(14.))
                    .text_color(theme.text)
            })))
            .child(icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Body)
                    .text_color(theme.text)
                    .child(name),
            )
            // The eye's slot is kept on rows without one, so every row is
            // the same height.
            .child(
                div()
                    .flex_none()
                    .size(px(Theme::BUTTON_HEIGHT))
                    .children(eye),
            )
            .on_click(cx.listener(move |_, _, _, cx| {
                external::choose(opens, path.clone());
                cx.notify();
            }))
            .into_any_element()
    }
}

/// The glyph column a settings row leads with, so rows under a head line up
/// with its title.
fn lead() -> bezel::gpui::Div {
    div()
        .flex_none()
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
}

fn default_icon(theme: &Theme) -> AnyElement {
    icons::icon(icons::glyph::Dock)
        .size(px(16.))
        .text_color(theme.text_muted)
        .into_any_element()
}
