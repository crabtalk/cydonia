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
        input::TextField,
        widgets::{Layout, Scaffolding},
    },
};
use std::collections::HashSet;

mod agents;
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
    Agents,
    Performance,
}

impl Section {
    const ALL: [Self; 3] = [Self::Appearance, Self::Agents, Self::Performance];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Agents => "Agents",
            Self::Performance => "Performance",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Appearance => icons::SUN,
            Self::Agents => icons::WIDGET,
            Self::Performance => icons::CPU,
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
                let mut this = SettingsWindow {
                    workspace,
                    section,
                    listings: None,
                    busy: HashSet::new(),
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
            Section::Appearance | Section::Performance => {}
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
                            .child(theme.page_header(self.section.title(), None))
                            .child(match self.section {
                                Section::Appearance => self.appearance_body(cx),
                                Section::Agents => self.agents_body(cx),
                                Section::Performance => self.performance_body(cx),
                            }),
                    ),
            )
            .children(self.cover_dialog(cx))
    }
}
