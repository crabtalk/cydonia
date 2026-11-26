use artifact::web::Viewport;
use bezel::{
    gpui::{self, App, Entity, MouseButton, Window, div, prelude::*, px, size},
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        icons::{Icon, glyph},
        menu::Item,
    },
};
use browser::{LoadState, Presentation, WebView};
use std::{cell::Cell, rc::Rc};

const INITIAL_DOCUMENT: &str = "about:blank";

#[derive(Clone, Default)]
pub(crate) struct Navigation(Rc<Cell<bool>>);

impl Navigation {
    pub(crate) fn update(&self, state: LoadState, page: &WebView) {
        // WebKit completes an initial about:blank navigation before opening
        // a local file with read access.
        self.0.set(
            state == LoadState::Finished
                && page.location().is_some_and(|url| url != INITIAL_DOCUMENT),
        );
    }

    pub(crate) fn reset(&self) {
        self.0.set(false);
    }

    pub(crate) fn ready(&self, page: &WebView) -> bool {
        self.0.get() && !page.is_loading() && page.can_evaluate()
    }
}

pub(crate) trait WebPresentation {
    fn present(
        &self,
        viewport: Viewport,
        interactive: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), String>;
}

impl WebPresentation for Entity<WebView> {
    fn present(
        &self,
        viewport: Viewport,
        interactive: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), String> {
        let viewport = gpui::native_view::Viewport::try_from(size(
            px(viewport.width as f32),
            px(viewport.height as f32),
        ))
        .map_err(|error| error.to_string())?;
        self.update(cx, |page, cx| {
            page.set_presentation(
                Some(Presentation {
                    viewport,
                    interactive,
                }),
                window,
                cx,
            )
        })
        .map_err(|error| error.to_string())
    }
}

/// How a viewport is shown: the device's mark, and its preset name or, for
/// any other size, its dimensions.
pub(crate) trait ViewportLook {
    fn icon(self) -> Icon;
    fn title(self) -> String;
}

impl ViewportLook for Viewport {
    fn icon(self) -> Icon {
        match self {
            Viewport::TABLET => glyph::Tablet.into(),
            Viewport::PHONE => glyph::Smartphone.into(),
            _ => glyph::Monitor.into(),
        }
    }

    fn title(self) -> String {
        self.name().map_or_else(|| self.to_string(), str::to_owned)
    }
}

pub(crate) fn viewport_items(current: Option<Viewport>) -> Vec<Item> {
    Viewport::PRESETS
        .into_iter()
        .map(|viewport| {
            Item::action(viewport.title())
                .with_icon(viewport.icon())
                .with_inline_description(viewport.to_string())
                .checked(current == Some(viewport))
        })
        .collect()
}

pub(crate) fn frame_header(title: &str, cx: &App) -> gpui::Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(-32.))
        .h(px(28.))
        .flex()
        .items_center()
        .gap(px(4.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_style(TextStyle::Callout)
                .text_color(Theme::of(cx).text_muted)
                .truncate()
                .child(title.to_owned()),
        )
}
