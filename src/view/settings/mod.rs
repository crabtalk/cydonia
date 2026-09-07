//! The settings window: a sidebar of sections, and the active section's body
//! centred under its title.
//!
//! A window rather than a sheet, and **opaque** rather than vibrant. Settings
//! is content, not chrome — a translucent panel would put the app you just
//! navigated away from directly behind the form you are filling in.

use crate::{
    agent::Listing,
    model::workspace::Workspace,
    view::root::{HEADER_HEIGHT, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y},
};
use bezel::{
    gpui::{
        App, Bounds, Context, Entity, Render, SharedString, TitlebarOptions, Window,
        WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions, div, point,
        prelude::*, px, size,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset, appearance},
    ui::{
        icons,
        input::{FieldEvent, Shape, TextField},
        widgets::{Layout, Scaffolding},
    },
};
use std::collections::HashSet;

mod agents;
mod features;
mod performance;
mod theme;
mod typography;

/// The section sidebar. The reference's 18rem is read against a 120rem panel;
/// against this window it would take a third of the width, so it matches the
/// main window's sidebar instead.
const SIDEBAR_WIDTH: f32 = 200.;

/// The gap between a group and the label of the next one, and between a label
/// and the box under it.
pub(super) const GROUP_GAP: f32 = 20.;
pub(super) const LABEL_GAP: f32 = 8.;

/// The reading column's cap, `--container-content`. The body is centred in
/// whatever the window gives it, up to this.
const CONTENT_MAX_WIDTH: f32 = 860.;

/// Which section the sidebar has selected.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Appearance,
    // Before Agents, because it is what decides whether agents matter: with
    // sessions off, nothing installed under Agents can be launched.
    Features,
    Agents,
    Performance,
}

impl Section {
    const ALL: [Self; 4] = [
        Self::Appearance,
        Self::Features,
        Self::Agents,
        Self::Performance,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Features => "Features",
            Self::Agents => "Agents",
            Self::Performance => "Performance",
        }
    }

    /// The line under the title, where the section needs one. It belongs to
    /// the header rather than the body: a subtitle sits with what it explains,
    /// and the gap under the whole block is the same either way.
    fn subtitle(self) -> Option<&'static str> {
        match self {
            Self::Features => Some("Parts of cydonia that stay off until you ask for them."),
            Self::Appearance | Self::Agents | Self::Performance => None,
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Appearance => icons::system::SUN,
            Self::Features => icons::system::TUNING,
            Self::Agents => icons::system::WIDGET,
            Self::Performance => icons::devices::CPU,
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
    /// What the agents section is being searched for. Held by the window
    /// rather than made where it is drawn: what has been typed has to outlive
    /// the frame, and a section is drawn afresh on every one.
    search: Entity<TextField>,
    /// The cover ceiling's field, while its dialog is up.
    editing: Option<Entity<TextField>>,
    error: Option<SharedString>,
}

/// Open the window, or bring the open one forward — a second settings window
/// would be two views of one preference.
pub fn open(
    workspace: Entity<Workspace>,
    existing: Option<WindowHandle<SettingsWindow>>,
    section: Section,
    cx: &mut App,
) -> Option<WindowHandle<SettingsWindow>> {
    if let Some(handle) = existing
        && handle
            .update(cx, |this, window, cx| {
                this.show(section, cx);
                window.activate_window();
            })
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
                let search = cx.new(|cx| {
                    TextField::new(cx)
                        .with_shape(Shape::Line)
                        .with_frame(false)
                        .with_placeholder("Search agents…")
                });
                // The list narrows as it is typed into. Subscribed rather than
                // observed: a field notifies on its own caret blink, and this
                // would rebuild the catalogue twice a second.
                cx.subscribe(&search, |_, _, event: &FieldEvent, cx| {
                    if *event == FieldEvent::Changed {
                        cx.notify();
                    }
                })
                .detach();
                let mut this = SettingsWindow {
                    workspace,
                    section,
                    listings: None,
                    busy: HashSet::new(),
                    search,
                    editing: None,
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
    /// What is on this machine can change while the window sits open —
    /// another install, a directory removed by hand — so the section's list is
    /// re-read on the way in rather than trusted from whenever it was opened.
    fn show(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        match section {
            Section::Agents => self.load(cx),
            Section::Appearance | Section::Features | Section::Performance => {}
        }
        cx.notify();
    }

    fn sidebar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let painter = Painter::of(cx);
        div()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(theme.surface)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(8.))
            .pb(px(8.))
            // Clears the traffic lights, which have no strip of their own.
            // Set after the shorthand — `p` writes every side.
            .pt(px(HEADER_HEIGHT))
            .children(Section::ALL.into_iter().enumerate().map(|(ix, section)| {
                theme
                    .nav_row(
                        Some(section.glyph()),
                        section.title(),
                        section == self.section,
                        Fade::new(painter, format!("section-{ix}")),
                    )
                    .id(("section", ix))
                    .on_click(cx.listener(move |this, _, _, cx| this.show(section, cx)))
            }))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(theme.bg)
            .font_family(theme.font_sans.clone())
            .text_color(theme.text)
            .text_style(TextStyle::Body)
            .child(self.sidebar(cx))
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
                            // The header block, held off its body by the gap
                            // that separates any two groups. Nothing set this
                            // before, so the page title leaned on `group_box`'s
                            // own margin and came out with less air under it
                            // than a field label gets — and none at all in a
                            // section that opens on a label rather than a box.
                            .child(
                                div()
                                    .mb(px(GROUP_GAP))
                                    .child(theme.page_header(self.section.title(), None))
                                    .children(
                                        self.section
                                            .subtitle()
                                            .map(|copy| theme.page_subtitle(copy)),
                                    ),
                            )
                            .child(match self.section {
                                Section::Appearance => self.appearance_body(cx),
                                Section::Features => self.features_body(cx),
                                Section::Agents => self.agents_body(cx),
                                Section::Performance => self.performance_body(cx),
                            }),
                    ),
            )
            .children(self.cover_dialog(cx))
    }
}
