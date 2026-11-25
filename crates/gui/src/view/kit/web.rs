use artifact::web::Viewport;
use bezel::gpui::{self, App, Entity, Window, px, size};
use browser::{Presentation, WebView};

pub(crate) fn present(
    page: &Entity<WebView>,
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
    page.update(cx, |page, cx| {
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
