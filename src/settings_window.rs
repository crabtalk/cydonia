//! The settings window: a rail of sections, and the active section's body
//! centred under its title.
//!
//! A window rather than a sheet, and **opaque** rather than frosted. Settings
//! is content, not chrome — a translucent panel would put the app you just
//! navigated away from directly behind the form you are filling in.

use crate::app::{Cydonia, TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y};
use bezel::{
    gpui::{
        App, Bounds, Context, Entity, Render, TitlebarOptions, Window, WindowBackgroundAppearance,
        WindowBounds, WindowHandle, WindowOptions, div, point, prelude::*, px, size,
    },
    motion::{Fade, Painter},
    theme::{
        Theme,
        appearance::{self, AppearanceMode},
    },
    ui::{
        icons,
        widgets::{Layout, Scaffolding},
    },
};

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

pub struct SettingsWindow {
    app: Entity<Cydonia>,
}

/// Open the window, or bring the open one forward — a second settings window
/// would be two views of one preference.
pub fn open(
    app: Entity<Cydonia>,
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
            cx.new(|_| SettingsWindow { app })
        },
    )
    .ok()
}

impl SettingsWindow {
    /// The sections. One for now, and the rail is here so the second costs a
    /// row rather than a layout.
    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            .child(
                theme
                    .nav_row(
                        Some(icons::SUN),
                        "Appearance",
                        true,
                        Fade::new(painter, "section-appearance"),
                    )
                    .id("section-appearance"),
            )
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
                                this.app.update(cx, |app, cx| app.set_appearance(mode, cx));
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
                            .child(theme.page_header("Appearance", None))
                            .child(theme.group_box().child(self.theme_row(cx))),
                    ),
            )
    }
}
