use crate::view::{design::DesignView, kit::web};
use artifact::{
    design::{Artboard, Design},
    web::Viewport,
};
use bezel::{
    gpui::{App, Context, Entity, Render, WeakEntity, Window, div, prelude::*, px},
    theme::{Glass, SurfaceStyle, Theme},
    ui::{
        context_menu::{ContextMenu, Place},
        icons, surface,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _, Controls as _},
    },
};

pub(super) struct ViewportButton {
    owner: WeakEntity<DesignView>,
    source: Entity<Design>,
    menu: ContextMenu<Self>,
}

impl ViewportButton {
    pub(super) fn new(
        owner: WeakEntity<DesignView>,
        source: Entity<Design>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        Self {
            owner,
            source,
            menu: ContextMenu::default(),
        }
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        ContextMenu::toggle(
            self,
            |this| &mut this.menu,
            (),
            Place::BelowEnd,
            |this, _, cx| {
                (
                    web::viewport_items(Some(this.source.read(cx).viewport)),
                    Self::choose,
                )
            },
            window,
            cx,
        );
    }

    fn choose(&mut self, path: &[usize], window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = path
            .first()
            .and_then(|index| Viewport::PRESETS.get(*index))
            .copied()
        else {
            return;
        };
        let _ = self.owner.update(cx, |view, cx| {
            view.defer_edit(
                window,
                cx,
                move |workspace, entry, cx| {
                    workspace.edit_design(
                        entry,
                        |design| {
                            design.viewport = preset;
                            Ok(())
                        },
                        cx,
                    )
                },
                |_, (), _| {},
            );
        });
    }
}

impl Render for ViewportButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.source.read(cx).viewport;
        let picker = cx.entity();
        let button = surface::of(
            "design-viewport-glass",
            SurfaceStyle::Glass(Glass::Regular),
            move |theme, _, cx| {
                picker.update(cx, |_, cx| {
                    let icon = icons::icon(icons::glyph::Monitor)
                        .size(px(14.))
                        .text_color(theme.text_muted);
                    let button = theme
                        .select_trigger_with(Some(icon), viewport.to_string())
                        .id("design-viewport")
                        .rounded(px(Theme::surface_radius()))
                        .border_0()
                        .py(px(0.))
                        .h(px(Theme::BUTTON_HEIGHT + 4.))
                        .w(px(162.))
                        .tooltip(|window, cx| Tooltip::text("Artboard viewport", window, cx))
                        .on_click(cx.listener(|this, _, window, cx| this.toggle(window, cx)));
                    ContextMenu::trigger(button, |this| &mut this.menu, (), cx)
                })
            },
        );
        div()
            .relative()
            .flex_none()
            .child(button)
            .children(
                self.menu
                    .render("design-viewport-menu", |this| &mut this.menu, window, cx),
            )
    }
}

impl DesignView {
    pub(super) fn comments_button(
        count: usize,
        cx: &App,
    ) -> bezel::gpui::Stateful<bezel::gpui::Div> {
        Theme::of(cx)
            .icon_button(icons::social::MessageCircle, ButtonStyle::Ghost, None)
            .id("artboard-comments")
            .flex_none()
            .w_auto()
            .px(px(6.))
            .gap(px(4.))
            .child(count.to_string())
            .tooltip(move |window, cx| Tooltip::text(format!("Comments ({count})"), window, cx))
    }

    pub(super) fn expanded_tools(
        &self,
        artboard: &Artboard,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = Theme::of(cx);
        let comment = if self.pins.picking {
            icons::glyph::X
        } else {
            icons::glyph::MessageCirclePlus
        };
        theme
            .control_group()
            .child(
                theme
                    .icon_button(comment, ButtonStyle::Ghost, None)
                    .id("artboard-pick")
                    .tooltip(|window, cx| Tooltip::text("Comment on an element", window, cx))
                    .when(self.open_browser().is_some(), |button| {
                        button.on_click(
                            cx.listener(|this, _, window, cx| this.toggle_picking(window, cx)),
                        )
                    }),
            )
            .child(
                Self::comments_button(artboard.comments.len(), cx)
                    .on_click(cx.listener(|this, _, _, cx| this.open_comments(cx))),
            )
            .child(
                theme
                    .icon_button(icons::arrows::Minimize2, ButtonStyle::Ghost, None)
                    .id("artboard-collapse")
                    .tooltip(|window, cx| Tooltip::text("Back to canvas", window, cx))
                    .on_click(cx.listener(|this, _, window, cx| this.close_artboard(window, cx))),
            )
    }
}
