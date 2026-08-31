//! The settings window: a sidebar of sections, and the active section's body
//! centred under its title.
//!
//! A window rather than a sheet, and **opaque** rather than frosted. Settings
//! is content, not chrome — a translucent panel would put the app you just
//! navigated away from directly behind the form you are filling in.

use crate::{
    agent::{Listing, mcp::McpServer},
    model::workspace::Workspace,
    view::root::{TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y},
};
use bezel::{
    gpui::{
        self, App, Bounds, Context, Entity, KeyBinding, Render, SharedString, TitlebarOptions,
        Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions, actions,
        div, point, prelude::*, px, size,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset, appearance},
    ui::{
        icons,
        input::TextField,
        widgets::{Layout, Scaffolding},
    },
};
use cacp_agents::mcp as registry;
use std::collections::HashSet;

mod agents;
mod mcp;
mod theme;

/// The section sidebar. The reference's 18rem is read against a 120rem panel;
/// against this window it would take a third of the width, so it matches the
/// main window's sidebar instead.
const SIDEBAR_WIDTH: f32 = 200.;

/// The reading column's cap, `--container-content`. The body is centred in
/// whatever the window gives it, up to this.
const CONTENT_MAX_WIDTH: f32 = 860.;

/// Claimed on the two MCP fields so `enter` runs the thing the field is for
/// and stays a newline everywhere else.
const SEARCH_CONTEXT: &str = "CydoniaMcpSearch";
const ADD_CONTEXT: &str = "CydoniaMcpAdd";

actions!(cydonia_settings, [SearchMcp, AddMcp]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", SearchMcp, Some(SEARCH_CONTEXT)),
        KeyBinding::new("enter", AddMcp, Some(ADD_CONTEXT)),
    ]);
}

/// Which section the sidebar has selected.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Appearance,
    Agents,
    // After Agents: a server is something an agent reaches, so it reads in the
    // order it is set up.
    Mcp,
}

impl Section {
    const ALL: [Self; 3] = [Self::Appearance, Self::Agents, Self::Mcp];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Agents => "Agents",
            Self::Mcp => "MCP servers",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Appearance => icons::SUN,
            Self::Agents => icons::WIDGET,
            Self::Mcp => icons::LINK,
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
    /// What `mcp.toml` holds. Re-read after every write rather than tracked —
    /// the file is the store, and one of them has to be the truth.
    configured: Vec<McpServer>,
    /// Registry hits for the last search, `None` before the first one — which
    /// is the difference between "nothing matched" and "you have not searched".
    results: Option<Vec<registry::Server>>,
    searching: bool,
    search: Entity<TextField>,
    add_name: Entity<TextField>,
    add_address: Entity<TextField>,
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
                let mut field = |context, placeholder| {
                    cx.new(|cx| {
                        TextField::new(cx)
                            .with_key_context(context)
                            .with_placeholder(placeholder)
                    })
                };
                let mut this = SettingsWindow {
                    workspace,
                    section: Section::Appearance,
                    listings: None,
                    busy: HashSet::new(),
                    error: None,
                    configured: Vec::new(),
                    results: None,
                    searching: false,
                    search: field(SEARCH_CONTEXT, "search the MCP registry…"),
                    add_name: field(ADD_CONTEXT, "name"),
                    add_address: field(ADD_CONTEXT, "command args… or https://…"),
                };
                this.reload();
                this.load(cx);
                this
            })
        },
    )
    .ok()
}

impl SettingsWindow {
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
                        match section {
                            // What is on this machine can change while the
                            // window sits open — another install, a directory
                            // removed by hand.
                            Section::Agents => this.load(cx),
                            Section::Mcp => this.reload(),
                            Section::Appearance => {}
                        }
                        cx.notify();
                    }))
            }))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        div()
            .size_full()
            .flex()
            .flex_row()
            .on_action(cx.listener(Self::search_mcp))
            .on_action(cx.listener(Self::add_mcp))
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
                                Section::Appearance => theme
                                    .group_box()
                                    .child(self.theme_row(cx))
                                    .into_any_element(),
                                Section::Agents => self.agents_body(cx),
                                Section::Mcp => self.mcp_body(cx),
                            }),
                    ),
            )
    }
}
