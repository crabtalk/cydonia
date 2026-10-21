//! The window's top band and its corners: the band's height and the line its
//! controls centre on, the corner controls the window draws once over
//! whichever row is at the top, the caption buttons and the drag handle.
//!
//! Every window opens with `appears_transparent` and `app_owns_titlebar_drag`,
//! and on Linux asks for client decorations, so nothing but a [`grip`] moves
//! it. Off macOS the caption buttons are the only ones it has.
//!
//! The rows that take turns at the window's top — the strip, the band over the
//! library, a pane's title row, the right panel's tabs — draw none of the
//! corner controls. Each leaves the room [`Corners`] names at the ends it
//! reaches, and [`Cydonia::corner_controls`] draws the controls in it.
//!
//! On macOS AppKit paints the lights. In a browser both are inert: the page
//! owns the frame the window sits in.

use crate::view::{
    component::panel::changes_toggle,
    window::root::{self, Cydonia},
};
use bezel::{
    gpui::{
        AnyElement, App, Context, Div, ElementId, Window, WindowDecorations, div, prelude::*, px,
    },
    theme::Theme,
    ui::{
        AppExt as _, tabs,
        titlebar::{self, CaptionSide, DragState},
    },
};

const NATIVE: bool = cfg!(any(target_os = "macos", target_family = "wasm"));
const WEB: bool = cfg!(target_family = "wasm");

/// What the band leaves above and below the strip's track of tabs.
const TRACK_GAP: f32 = 5.;
/// The fold and back and forward, three icon buttons.
const FOLD_WIDTH: f32 = 3. * Theme::BUTTON_HEIGHT;
/// The panel's toggle, one icon button.
const TOGGLE_WIDTH: f32 = Theme::BUTTON_HEIGHT;
/// Between a corner's controls and the row's own content.
const CORNER_GAP: f32 = 8.;

/// How tall the window's top band stands: taller while the strip shows its
/// tabs.
pub fn top_height(tabs: bool) -> f32 {
    match tabs {
        true => tabs::TRACK_HEIGHT + 2. * TRACK_GAP,
        false => root::HEADER_HEIGHT,
    }
}

/// The window's top band, `tabs` saying whether the strip shows its tabs,
/// its contents centred down it.
pub fn top_band(tabs: bool) -> Div {
    root::band().h(px(top_height(tabs)))
}

/// A card under a row of the window's top band: bezel's content card with
/// nothing above it but that row.
pub fn card(floating: bool, cx: &App) -> Div {
    bezel::ui::window::content(floating, cx).mt_0()
}

/// Which of the window's top corners a row along its top edge reaches: the
/// left one under the lights, the left caption buttons and the sidebar's fold,
/// the right one under the panel's toggle and the right caption buttons.
/// `tabs` is whether the strip shows its tabs, which sets the band the lights
/// are centred down — see [`top_height`].
#[derive(Clone, Copy)]
pub struct Corners {
    pub left: bool,
    pub right: bool,
    pub tabs: bool,
}

impl Corners {
    /// The room the row leaves at its start: what the window draws at its
    /// top-left corner, or `inside` where the row does not reach it.
    pub fn lead(self, inside: f32, window: &Window, cx: &App) -> f32 {
        match self.left {
            false => inside,
            true => left_inset(window, top_height(self.tabs), cx) + FOLD_WIDTH + CORNER_GAP,
        }
    }

    /// The room the row leaves at its end: what the window draws at its
    /// top-right corner, or `inside` where the row does not reach it.
    pub fn trail(self, inside: f32, window: &Window, cx: &App) -> f32 {
        match self.right {
            false => inside,
            true => right_inset(window, cx) + TOGGLE_WIDTH + CORNER_GAP,
        }
    }
}

/// Where the top-left corner's controls start under a top band `band` tall:
/// flush with the left caption buttons, or past the lights.
fn left_inset(window: &Window, band: f32, cx: &App) -> f32 {
    match has(CaptionSide::Left, window, cx) {
        true => caption_width(CaptionSide::Left, window, cx),
        false => root::toolbar_inset(window, band, cx),
    }
}

/// How far the top-right corner's controls end from the window's right edge.
fn right_inset(window: &Window, cx: &App) -> f32 {
    match has(CaptionSide::Right, window, cx) {
        true => caption_width(CaptionSide::Right, window, cx),
        false => root::BUTTON_EDGE,
    }
}

/// How wide the caption buttons on `side` stand.
fn caption_width(side: CaptionSide, window: &Window, cx: &App) -> f32 {
    if !has(side, window, cx) {
        return 0.;
    }
    let layout = cx
        .caption_style()
        .button_layout(cx.button_layout(), window.window_controls());
    let buttons = match side {
        CaptionSide::Left => layout.left,
        CaptionSide::Right => layout.right,
    };
    buttons.iter().flatten().count() as f32 * Theme::CAPTION_BUTTON_WIDTH
}

impl Cydonia {
    /// Whether the window's top row is the strip showing its tabs.
    pub(crate) fn tabs_on_top(&self, cx: &App) -> bool {
        self.library.is_none()
            && !self.panes_on_top(cx)
            && self.workspace.read(cx).arranged().is_some()
            && self.strip.read(cx).tracked(cx)
    }

    /// The window's corner controls, over whichever row is at its top: the
    /// left caption buttons and the sidebar's fold with back and forward past
    /// the lights, and the panel's toggle and the right caption buttons at
    /// the right.
    pub(crate) fn corner_controls(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let band = top_height(self.tabs_on_top(cx));
        let toggle = (self.changes.is_some() || self.shell_cwd(cx).is_some())
            .then(|| changes_toggle(self.changes.is_some(), cx));
        let corner = || {
            div()
                .absolute()
                .top_0()
                .h(px(band))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(CORNER_GAP))
                .occlude()
        };
        let left = corner()
            .left_0()
            .pl(px(match has(CaptionSide::Left, window, cx) {
                true => 0.,
                false => root::toolbar_inset(window, band, cx),
            }))
            .children(caption(CaptionSide::Left, window, cx))
            .child(self.fold_controls(cx));
        let right = corner()
            .right_0()
            .pr(px(match has(CaptionSide::Right, window, cx) {
                true => 0.,
                false => root::BUTTON_EDGE,
            }))
            .children(toggle)
            .children(caption(CaptionSide::Right, window, cx));
        div()
            .absolute()
            .inset_0()
            .child(left)
            .child(right)
            .into_any_element()
    }
}

/// The caption buttons for `side`, or nothing where [`has`] says none.
pub fn caption(side: CaptionSide, window: &Window, cx: &App) -> Option<AnyElement> {
    has(side, window, cx).then(|| {
        titlebar::controls(side, window, cx)
            .flex_none()
            .into_any_element()
    })
}

/// The free stretch of a band. It drags the window, zooms it on a double
/// click and, off macOS, opens the window menu on a right press.
pub fn grip(id: impl Into<ElementId>, drag: &DragState, window: &Window) -> AnyElement {
    match WEB {
        true => div().flex_1().min_w_0().into_any_element(),
        false => titlebar::grip(id, drag, window)
            .min_w_0()
            .into_any_element(),
    }
}

/// Whether [`caption`] draws buttons on `side`: never on macOS, in a browser
/// or in full screen. Traffic lights are always on the left; the rectangular
/// buttons go on the side the desktop's layout names, or all on the right
/// where there is no layout. A band that has them
/// drops its inset on that side, so the buttons sit flush with the window's
/// edge.
pub fn has(side: CaptionSide, window: &Window, cx: &App) -> bool {
    if NATIVE || window.is_fullscreen() {
        return false;
    }
    let layout = Some(
        cx.caption_style()
            .button_layout(cx.button_layout(), window.window_controls()),
    );
    match side {
        CaptionSide::Left => layout.is_some_and(|layout| layout.left[0].is_some()),
        CaptionSide::Right => layout.is_none_or(|layout| layout.right[0].is_some()),
    }
}

/// What a window asks the desktop for. Client on Linux, so the frame and the
/// buttons are the same under every compositor; ignored elsewhere.
pub fn decorations() -> Option<WindowDecorations> {
    cfg!(any(target_os = "linux", target_os = "freebsd")).then_some(WindowDecorations::Client)
}
