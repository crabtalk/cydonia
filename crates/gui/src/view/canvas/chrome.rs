use crate::view::{
    canvas::{CanvasView, Menu, actions::Scope},
    kit::{
        canvas::{Navigation, TOOL_PADDING},
        menu::{self, MenuHost as _},
    },
};
use bezel::{
    gpui::{AnyElement, Context, Entity, Render, WeakEntity, Window, div, prelude::*, px},
    theme::{ControlSize, Theme},
    ui::{
        context_menu::Place,
        icons,
        tooltip::Tooltip,
        widgets::{Button, Scaffolding as _},
    },
};
use canvas::CanvasView as Surface;

pub(super) const HEIGHT: f32 = ControlSize::Regular.height() + 2. * (TOOL_PADDING + 1.);

pub(super) struct Chrome {
    owner: WeakEntity<CanvasView>,
    read_only: bool,
}
impl Chrome {
    pub(super) fn new(
        owner: WeakEntity<CanvasView>,
        surface: &Entity<Surface>,
        cx: &mut Context<Self>,
    ) -> Self {
        if let Some(owner) = owner.upgrade() {
            cx.observe(&owner, |_, _, cx| cx.notify()).detach();
        }
        cx.observe(surface, |this, surface, cx| {
            let read_only = surface.read(cx).editor().options().read_only;
            if this.read_only != read_only {
                this.read_only = read_only;
                cx.notify();
            }
        })
        .detach();
        Self {
            owner,
            read_only: surface.read(cx).editor().options().read_only,
        }
    }
}
impl Render for Chrome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner
            .update(cx, |owner, cx| owner.tools(window, cx))
            .unwrap_or_else(|_| div().into_any_element())
    }
}
impl CanvasView {
    fn tools(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let read_only = self.surface.read(cx).editor().options().read_only;
        let actions = menu::button(
            "canvas-actions",
            icons::account::Settings,
            menu::Reveal::Always,
            self.menu.key() == Some(&Menu::Actions),
            cx,
        )
        .tooltip(|window, cx| Tooltip::text("Canvas actions", window, cx))
        .on_click(cx.listener(|this, _, window, cx| {
            this.at = this.surface.read(cx).editor().center();
            this.toggle_menu(
                Menu::Actions,
                Place::Above,
                |this, _, cx| this.rows(Scope::Actions, cx),
                window,
                cx,
            );
        }));
        let actions = self
            .menu_trigger(actions, Menu::Actions, cx)
            .children(self.menu_here(&Menu::Actions, "canvas-actions-menu", window, cx));
        let tools = theme
            .group_box()
            .flex_row()
            .p(px(TOOL_PADDING))
            .gap(px(2.))
            .child(
                Button::new("canvas-add-text", "Text")
                    .icon(icons::text::Type)
                    .enabled(!read_only)
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.at = this.surface.read(cx).editor().center();
                        this.add_text(window, cx);
                    })),
            )
            .child(
                Button::new("canvas-add-group", "Group")
                    .icon(icons::layout::PanelsTopLeft)
                    .enabled(!read_only)
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.at = this.surface.read(cx).editor().center();
                        this.add_group(window, cx);
                    })),
            )
            .child(actions);
        div()
            .w_full()
            .flex()
            .justify_between()
            .gap(px(8.))
            .child(tools)
            .child(Navigation(self.surface.clone()).render(cx))
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/canvas_chrome.rs"]
mod tests;
