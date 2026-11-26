use bezel::{
    gpui::{App, Entity, Focusable as _, IntoElement, Window, div, prelude::*, px},
    theme::{Glass, SurfaceStyle, Theme},
    ui::{icons, surface, widgets::Button},
};
use canvas::{CanvasEditor, CanvasView};

pub(crate) const TOOL_PADDING: f32 = 4.;

pub(crate) struct Navigation(pub Entity<CanvasView>);

impl Navigation {
    pub(crate) fn render(self) -> impl IntoElement {
        let tools = [
            (
                "canvas-zoom-out",
                "Zoom out",
                icons::glyph::Minus,
                CanvasEditor::zoom_out as fn(&mut CanvasEditor),
            ),
            (
                "canvas-fit",
                "Zoom to fit",
                icons::layout::Maximize,
                CanvasEditor::fit,
            ),
            (
                "canvas-zoom-in",
                "Zoom in",
                icons::math::Plus,
                CanvasEditor::zoom_in,
            ),
        ];
        surface::of(
            "canvas-navigation",
            SurfaceStyle::Glass(Glass::Regular),
            move |_, _, _| {
                div()
                    .rounded(px(Theme::surface_radius()))
                    .flex()
                    .flex_row()
                    .p(px(TOOL_PADDING + 1.))
                    .gap(px(2.))
                    .children(tools.map(|(id, label, icon, command)| {
                        let surface = self.0.clone();
                        Button::new(id, label).icon(icon).label_hidden().on_press(
                            move |_, window, cx| {
                                Self::apply(&surface, command, window, cx);
                            },
                        )
                    }))
            },
        )
    }

    pub(crate) fn apply(
        surface: &Entity<CanvasView>,
        command: fn(&mut CanvasEditor),
        window: &mut Window,
        cx: &mut App,
    ) {
        surface.update(cx, |surface, cx| surface.update_editor(cx, command));
        window.focus(&surface.read(cx).focus_handle(cx), cx);
    }
}
