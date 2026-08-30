//! The settings window: a rail of sections, and the active section's body
//! centred under its title.
//!
//! A window rather than a sheet, and **opaque** rather than frosted. Settings
//! is content, not chrome — a translucent panel would put the app you just
//! navigated away from directly behind the form you are filling in.

use crate::{
    agents::{self, Listing},
    model::workspace::Workspace,
    view::root::{TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y},
};
use bezel::{
    gpui::{
        AnyElement, App, Bounds, Context, Entity, Render, SharedString, TitlebarOptions, Window,
        WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions, div, point,
        prelude::*, px, size, svg,
    },
    motion::{Fade, Painter},
    theme::{
        Theme,
        appearance::{self, AppearanceMode},
    },
    ui::{
        icons,
        widgets::{ButtonStyle, Buttons, Content, Layout, Scaffolding, Status},
    },
};
use std::collections::HashSet;

/// The section rail. The reference's 18rem is read against a 120rem panel;
/// against this window it would take a third of the width, so it matches the
/// main window's rail instead.
const RAIL_WIDTH: f32 = 200.;

/// The reading column's cap, `--container-content`. The body is centred in
/// whatever the window gives it, up to this.
const CONTENT_MAX_WIDTH: f32 = 860.;

const MODES: [AppearanceMode; 3] = [
    AppearanceMode::System,
    AppearanceMode::Light,
    AppearanceMode::Dark,
];

/// Which section the rail has selected.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Agents,
}

impl Section {
    const ALL: [Self; 2] = [Self::Appearance, Self::Agents];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Agents => "Agents",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Appearance => icons::SUN,
            Self::Agents => icons::WIDGET,
        }
    }
}

pub struct SettingsWindow {
    workspace: Entity<Workspace>,
    section: Section,
    /// The catalog, once it has been fetched. `None` while it is in flight —
    /// which is the difference between "still looking" and "nothing here".
    listings: Option<Vec<Listing>>,
    /// Agents with an install or a removal running.
    busy: HashSet<String>,
    error: Option<SharedString>,
}

/// Open the window, or bring the open one forward — a second settings window
/// would be two views of one preference.
pub fn open(
    workspace: Entity<Workspace>,
    existing: Option<WindowHandle<SettingsWindow>>,
    cx: &mut App,
) -> Option<WindowHandle<SettingsWindow>> {
    if let Some(handle) = existing
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let bounds = Bounds::centered(None, size(px(900.), px(620.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                appears_transparent: true,
                traffic_light_position: Some(point(px(TRAFFIC_LIGHT_X), px(TRAFFIC_LIGHT_Y))),
                ..Default::default()
            }),
            // Opaque on purpose — see the module note.
            window_background: WindowBackgroundAppearance::Opaque,
            app_id: Some("cydonia-settings".into()),
            ..Default::default()
        },
        |window, cx| {
            appearance::observe_window(window, cx).detach();
            cx.new(|cx| {
                let mut this = SettingsWindow {
                    workspace,
                    section: Section::Appearance,
                    listings: None,
                    busy: HashSet::new(),
                    error: None,
                };
                this.load(cx);
                this
            })
        },
    )
    .ok()
}

impl SettingsWindow {
    /// Fetch the catalog and each agent's local state, off the UI thread.
    fn load(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let listings = cx
                .background_executor()
                .spawn(async move { agents::listings() })
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
                    agents::prefetch_icons();
                    agents::listings()
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
                .spawn(async move { agents::install(&agent) })
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
                .spawn(async move { agents::remove(&id) })
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

    fn rail(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        div()
            .flex_none()
            .w(px(RAIL_WIDTH))
            .h_full()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .px(px(8.))
            .pb(px(8.))
            // Clears the traffic lights, which have no strip of their own.
            // Set after the shorthand — `p` writes every side.
            .pt(px(Theme::HEADER_HEIGHT))
            .children(Section::ALL.into_iter().enumerate().map(|(ix, section)| {
                theme
                    .nav_row(
                        Some(section.glyph()),
                        section.title(),
                        section == self.section,
                        Fade::new(painter, format!("section-{ix}")),
                    )
                    .id(("section", ix))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        // What is on this machine can change while the window
                        // sits open — another install, a directory removed by
                        // hand — so the list is re-read on the way in rather
                        // than trusted from whenever the window was opened.
                        if section == Section::Agents {
                            this.load(cx);
                        }
                        cx.notify();
                    }))
            }))
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
                            .text_size(px(11.5))
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
                    .text_size(px(12.))
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
                    .text_size(px(12.))
                    .text_color(theme.text_faint)
                    .child("unavailable")
                    .into_any_element(),
            })
            .into_any_element()
    }

    /// The agents section: everything the registry publishes, installed first
    /// so what you already have is what you see.
    fn agents_body(&self, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(listings) = self.listings.as_ref() else {
            return theme
                .group_box()
                .child(
                    theme.card_row(true).child(
                        div()
                            .text_size(px(12.5))
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

    /// One card row: what the setting is on the left, the control on the right.
    fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let current = appearance::mode(cx);
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Theme"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(11.5))
                            .text_color(theme.text_muted)
                            .child("Follow the system, or pick one."),
                    ),
            )
            .child(
                // A segmented control rather than a select: three options that
                // all fit are worth showing at once.
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(Theme::button_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .children(MODES.into_iter().enumerate().map(|(ix, mode)| {
                        let selected = mode == current;
                        div()
                            .id(("appearance", ix))
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .text_size(px(12.5))
                            .cursor_pointer()
                            .when(selected, |el| {
                                el.bg(theme.element_active).text_color(theme.text)
                            })
                            .when(!selected, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.element_hover))
                            })
                            .child(mode.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.set_appearance(mode, cx));
                                cx.notify();
                            }))
                    })),
            )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(theme.bg)
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_size(px(14.))
            .child(self.rail(cx))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px(px(32.))
                    .py(px(32.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(CONTENT_MAX_WIDTH))
                            .flex()
                            .flex_col()
                            .child(theme.page_header(self.section.title(), None))
                            .child(match self.section {
                                Section::Appearance => theme
                                    .group_box()
                                    .child(self.theme_row(cx))
                                    .into_any_element(),
                                Section::Agents => self.agents_body(cx),
                            }),
                    ),
            )
    }
}
