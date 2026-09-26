//! The settings window: a sidebar of sections, and the active section's body
//! centred under its title.
//!
//! A window rather than a sheet, and **opaque** rather than vibrant. Settings
//! is content, not chrome — a translucent panel would put the app you just
//! navigated away from directly behind the form you are filling in.
//!
//! Without the `desktop` feature there is one window, and this is drawn inside
//! it — see [`embed`].

#[cfg(feature = "desktop")]
use crate::{
    agent::Listing,
    model::update,
    view::root::{TRAFFIC_LIGHT_X, TRAFFIC_LIGHT_Y},
};
use crate::{model::workspace::Workspace, view::root::HEADER_HEIGHT};
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        AnyElement, App, Context, ElementId, Entity, Render, SharedString, Window, div, prelude::*,
        px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
        widgets::{Content, Controls, Layout, Scaffolding},
    },
};
#[cfg(feature = "desktop")]
use bezel::{
    gpui::{
        Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowHandle,
        WindowOptions, point, size,
    },
    theme::appearance,
    ui::input::{FieldEvent, Shape},
};
#[cfg(feature = "desktop")]
use std::collections::{HashMap, HashSet};

#[cfg(feature = "desktop")]
mod agents;
#[cfg(not(feature = "desktop"))]
#[path = "agents_web.rs"]
mod agents;
mod developer;
mod features;
mod general;
mod mcp;
mod performance;
mod shortcuts;
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

pub use super::section::Section;

impl Section {
    /// Whether this section carries its own scroller, leaving the page still.
    ///
    /// Only where a section holds a list long enough to be worth building by
    /// the viewport — see [`SettingsWindow::agents_list`]. Everything else is
    /// a page of boxes, and the page scrolls it.
    pub(super) fn owns_scroll(&self) -> bool {
        matches!(self, Self::Agents)
    }

    const ALL: [Self; 8] = [
        Self::General,
        Self::Appearance,
        Self::Shortcuts,
        Self::Features,
        Self::Agents,
        Self::Mcp,
        Self::Performance,
        Self::Developer,
    ];

    /// Whether this build lists it in the sidebar. Developer holds switches for
    /// looking at what has not happened yet, which is not something to hand
    /// somebody who installed the app — so it is absent from a release build
    /// rather than empty in one, and every build anyone installs is a release
    /// one.
    ///
    /// The `developer` feature is the other way in, for what the updater
    /// switches need: the updater runs in a bundle and nowhere else, and
    /// `make bundle FEATURES=developer` is that bundle built at the profile
    /// that ships rather than at `debug`.
    fn listed(self) -> bool {
        !matches!(self, Self::Developer) || cfg!(debug_assertions) || cfg!(feature = "developer")
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Shortcuts => "Shortcuts",
            Self::Features => "Features",
            Self::Agents => "Agents",
            Self::Mcp => "MCP",
            Self::Performance => "Performance",
            Self::Developer => "Developer",
        }
    }

    /// The line under the title, where the section needs one. It belongs to
    /// the header rather than the body: a subtitle sits with what it explains,
    /// and the gap under the whole block is the same either way.
    fn subtitle(self) -> Option<&'static str> {
        match self {
            Self::Shortcuts => {
                Some("Press a chord to record it. ⎋ leaves it alone, ⌫ takes it away.")
            }
            Self::Features => Some("Parts of cydonia you can put away, and ones to ask for."),
            Self::Mcp => {
                Some("The tools cydonia offers the agents it runs, over a port on this machine.")
            }
            Self::Developer => Some("Switches for looking at what has not happened yet."),
            Self::General | Self::Appearance | Self::Agents | Self::Performance => None,
        }
    }

    fn glyph(self) -> &'static [u8] {
        match self {
            // The gear macOS itself puts on General.
            Self::General => icons::account::Settings,
            Self::Appearance => icons::weather::Sun,
            Self::Shortcuts => icons::development::Command,
            Self::Features => icons::account::SlidersHorizontal,
            Self::Agents => icons::development::Bot,
            Self::Mcp => icons::development::Plug,
            Self::Performance => icons::devices::Cpu,
            Self::Developer => icons::development::Wrench,
        }
    }
}

pub struct SettingsWindow {
    workspace: Entity<Workspace>,
    /// The press on the window's [`crate::view::chrome::grip`].
    drag: bezel::ui::titlebar::DragState,
    section: Section,
    /// The catalog, once it has been fetched. `None` while it is in flight —
    /// which is the difference between "still looking" and "nothing here".
    #[cfg(feature = "desktop")]
    listings: Option<Vec<Listing>>,
    /// Agents with an install or a removal running.
    #[cfg(feature = "desktop")]
    busy: HashSet<String>,
    /// The row whose install is waiting to be agreed to — see
    /// [`SettingsWindow::trust_dialog`].
    #[cfg(feature = "desktop")]
    trusting: Option<usize>,
    /// The catalogue's rows, built only where they are on screen.
    ///
    /// The registry publishes dozens, each a row of a dozen elements, and a
    /// page that hands all of them to the layout engine lays every one of them
    /// out on every frame a scroll draws — which is where this section's time
    /// went, measured. Keyed by agent id, so narrowing the search reconciles
    /// against what is on screen rather than scrolling it.
    #[cfg(feature = "desktop")]
    agents_list: bezel::ui::list::VariableList<String>,
    /// What the installer has printed for each of them, newest last: the tail
    /// is the row's status while it runs, and the whole of it is all a failure
    /// has to explain itself with — see [`crate::agent::record`].
    #[cfg(feature = "desktop")]
    output: HashMap<String, Vec<String>>,
    /// What the agents section is being searched for. Held by the window
    /// rather than made where it is drawn: what has been typed has to outlive
    /// the frame, and a section is drawn afresh on every one.
    #[cfg(feature = "desktop")]
    search: Entity<TextField>,
    /// The family pickers, held for the same reason the search field is: the
    /// menu one of them has open has to outlive the frame it was opened in.
    interface_font: typography::FamilyPicker,
    article_font: typography::FamilyPicker,
    mono_font: typography::FamilyPicker,
    /// The cover ceiling's field, while its dialog is up.
    editing: Option<Entity<TextField>>,
    /// The shortcut row taking keys, while one is — see
    /// [`shortcuts::Recording`].
    recording: Option<shortcuts::Recording>,
    #[cfg(feature = "desktop")]
    error: Option<SharedString>,
}

/// Open the window, or bring the open one forward — a second settings window
/// would be two views of one preference.
#[cfg(feature = "desktop")]
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
            // Opaque on purpose — see the module note. The root paints the
            // page's own background, so where the window frames itself the
            // surface is transparent and only the frame's band shows through.
            window_background: match crate::view::chrome::decorations() {
                Some(_) => WindowBackgroundAppearance::Transparent,
                None => WindowBackgroundAppearance::Opaque,
            },
            app_id: Some("cydonia".into()),
            window_decorations: crate::view::chrome::decorations(),
            ..Default::default()
        },
        |window, cx| {
            appearance::observe_window(window, cx).detach();
            // And opaque it stays: bezel pushes the palette's own background
            // onto every window on each appearance switch, which is what keeps
            // the main window's frost alive and would frost this one with it.
            appearance::keep_background(window, cx);
            cx.new(|cx| SettingsWindow::new(workspace, section, cx))
        },
    )
    .ok()
}

/// The same page, drawn inside the one window there is.
#[cfg(not(feature = "desktop"))]
pub fn embed(
    workspace: Entity<Workspace>,
    section: Section,
    cx: &mut App,
) -> Entity<SettingsWindow> {
    cx.new(|cx| SettingsWindow::new(workspace, section, cx))
}

impl SettingsWindow {
    fn new(workspace: Entity<Workspace>, section: Section, cx: &mut Context<Self>) -> Self {
        #[cfg(feature = "desktop")]
        let search = cx.new(|cx| {
            TextField::new(cx)
                .with_shape(Shape::Line)
                .with_frame(false)
                .with_placeholder("Search agents…")
        });
        // The list narrows as it is typed into. Subscribed rather than
        // observed: a field notifies on its own caret blink, and this
        // would rebuild the catalogue twice a second.
        #[cfg(feature = "desktop")]
        cx.subscribe(&search, |_, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                cx.notify();
            }
        })
        .detach();
        // The general section reads the updater, which moves on its own
        // — a check that lands while this window sits open has to reach
        // the row that reports it.
        #[cfg(feature = "desktop")]
        if let Some(updater) = update::of(cx) {
            cx.observe(&updater, |_, _, cx| cx.notify()).detach();
        }
        let fonts = workspace.read(cx).fonts.clone();
        let interface_font =
            typography::FamilyPicker::new(typography::Face::Interface, fonts.sans, cx);
        let article_font = typography::FamilyPicker::new(typography::Face::Article, fonts.body, cx);
        let mono_font = typography::FamilyPicker::new(typography::Face::Mono, fonts.mono, cx);
        let mut this = SettingsWindow {
            drag: Default::default(),
            workspace,
            section,
            #[cfg(feature = "desktop")]
            listings: None,
            #[cfg(feature = "desktop")]
            busy: HashSet::new(),
            #[cfg(feature = "desktop")]
            trusting: None,
            #[cfg(feature = "desktop")]
            agents_list: {
                let list = bezel::ui::list::VariableList::default();
                // A catalogue is read from the top. Following the tail
                // is the transcript's rule, and what this comes set to.
                list.state.set_follow_mode(bezel::gpui::FollowMode::Normal);
                list
            },
            #[cfg(feature = "desktop")]
            output: HashMap::new(),
            #[cfg(feature = "desktop")]
            search,
            interface_font,
            article_font,
            mono_font,
            editing: None,
            recording: None,
            #[cfg(feature = "desktop")]
            error: None,
        };
        this.load(cx);
        // The keymap is emptied while a chord is being recorded, so a
        // window shut in the middle of that has to put it back — see
        // [`shortcuts`].
        cx.on_release(|this: &mut SettingsWindow, cx| {
            if this.recording.is_some() {
                shortcuts::restore(&this.workspace, cx);
            }
        })
        .detach();
        this
    }
}

/// One row of a settings group that carries a switch: an optional icon, a
/// title over a line of explanation, and the toggle on the right.
///
/// Six sections built this row by hand and they had drifted apart in nothing
/// but their copy, so it lives here and they pass what differs.
pub(super) struct Switch {
    id: ElementId,
    title: SharedString,
    blurb: SharedString,
    on: bool,
    first: bool,
    glyph: Option<&'static [u8]>,
    truncate: bool,
    badge: Option<SharedString>,
}

impl Switch {
    pub(super) fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        blurb: impl Into<SharedString>,
        on: bool,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            blurb: blurb.into(),
            on,
            first: false,
            glyph: None,
            truncate: false,
            badge: None,
        }
    }

    /// Whether this is the first row of its group box — `card_row` draws no
    /// divider above that one.
    pub(super) fn first(mut self, first: bool) -> Self {
        self.first = first;
        self
    }

    /// The mark down the left, which the dense lists carry and a row standing
    /// on its own does not.
    pub(super) fn glyph(mut self, glyph: &'static [u8]) -> Self {
        self.glyph = Some(glyph);
        self
    }

    /// Hold the blurb to one line, whatever the window is doing: a row that
    /// grows a second one moves every switch below it down the column.
    pub(super) fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }

    pub(super) fn badge(mut self, badge: Option<impl Into<SharedString>>) -> Self {
        self.badge = badge.map(Into::into);
        self
    }
}

impl SettingsWindow {
    /// Paint a [`Switch`], calling `flip` when it is pressed.
    pub(super) fn switch_row(
        &self,
        switch: Switch,
        cx: &Context<Self>,
        flip: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(switch.first)
            .children(switch.glyph.map(|glyph| {
                div()
                    .flex_none()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        icons::icon(glyph)
                            .size(px(16.))
                            .flex_none()
                            .text_color(theme.text_muted),
                    )
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(switch.title))
                    .child(
                        div()
                            .mt(px(4.))
                            .when(switch.truncate, |el| el.truncate())
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(switch.blurb),
                    ),
            )
            .children(switch.badge.map(|label| theme.badge(label)))
            .child(
                div()
                    .id(switch.id)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        flip(this, cx);
                        cx.notify();
                    }))
                    .child(theme.toggle(switch.on)),
            )
            .into_any_element()
    }
}

impl SettingsWindow {
    /// What is on this machine can change while the window sits open —
    /// another install, a directory removed by hand — so the section's list is
    /// re-read on the way in rather than trusted from whenever it was opened.
    fn show(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        match section {
            Section::Agents => self.load(cx),
            Section::General
            | Section::Appearance
            | Section::Shortcuts
            | Section::Features
            | Section::Mcp
            | Section::Performance
            | Section::Developer => {}
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
            .pt(px(match cfg!(feature = "desktop") {
                true => HEADER_HEIGHT,
                false => 12.,
            }))
            .children(
                Section::ALL
                    .into_iter()
                    .filter(|section| section.listed())
                    .enumerate()
                    .map(|(ix, section)| {
                        theme
                            .nav_row(
                                Some(section.glyph().into()),
                                section.title(),
                                section == self.section,
                                Fade::new(painter, format!("section-{ix}")),
                            )
                            .id(("section", ix))
                            .on_click(cx.listener(move |this, _, _, cx| this.show(section, cx)))
                    }),
            )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::view::chrome;
        use bezel::ui::titlebar::CaptionSide;
        let theme = Theme::of(cx).clone();
        let owns_scroll = self.section.owns_scroll();
        // Off macOS the window's top edge is a strip of its own, over the
        // sidebar's empty band and the page's top margin.
        let strip = (!cfg!(target_os = "macos") && cfg!(feature = "desktop")).then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(HEADER_HEIGHT))
                .flex()
                .flex_row()
                .children(chrome::caption(CaptionSide::Left, window, cx))
                .child(chrome::grip("settings-grip", &self.drag, window))
                .children(chrome::caption(CaptionSide::Right, window, cx))
        });
        let root = div()
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
                    // A section that scrolls itself keeps the page still: its
                    // list is the scroller, and a page scrolling behind one
                    // would be two bars down one column.
                    .when(owns_scroll, |el| el.overflow_hidden())
                    .when(!owns_scroll, |el| el.overflow_y_scroll())
                    .px(px(32.))
                    .py(px(32.))
                    .when(strip.is_some(), |el| el.pt(px(HEADER_HEIGHT)))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(CONTENT_MAX_WIDTH))
                            .when(owns_scroll, |el| el.flex_1().min_h_0())
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
                                    // Chrome above a section that scrolls
                                    // itself: it keeps its height and the list
                                    // below takes what is left.
                                    .when(owns_scroll, |el| el.flex_none())
                                    .child(theme.page_header(self.section.title(), None))
                                    .children(
                                        self.section
                                            .subtitle()
                                            .map(|copy| theme.page_subtitle(copy)),
                                    ),
                            )
                            .child(match self.section {
                                Section::General => self.general_body(cx),
                                Section::Appearance => self.appearance_body(cx),
                                Section::Shortcuts => self.shortcuts_body(cx),
                                Section::Features => self.features_body(cx),
                                Section::Agents => self.agents_body(cx),
                                Section::Mcp => self.mcp_body(cx),
                                Section::Performance => self.performance_body(cx),
                                Section::Developer => self.developer_body(cx),
                            }),
                    )
                    .map(|pane| match owns_scroll {
                        // The list brings its own bar.
                        true => pane.into_any_element(),
                        false => scrollbars::Viewport::new(
                            "settings-scroll",
                            pane,
                            bezel::gpui::Axis::Vertical,
                        )
                        .fill()
                        .into_any_element(),
                    }),
            )
            .children(strip)
            .children(self.cover_dialog(cx))
            .children(self.trust_dialog(cx));
        #[cfg(feature = "desktop")]
        return bezel::ui::window::frame(root, window, cx);
        #[cfg(not(feature = "desktop"))]
        root
    }
}
