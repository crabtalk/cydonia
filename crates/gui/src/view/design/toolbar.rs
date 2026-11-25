use crate::view::{
    design::{DesignView, GUTTER},
    kit::web,
};
use artifact::design::{Artboard, Design};
use bezel::{
    gpui::{App, Context, Entity, Render, WeakEntity, Window, div, prelude::*, px},
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};

pub(crate) struct ViewportButton {
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
}

impl Render for ViewportButton {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.source.read(cx).viewport;
        let menu = self
            .menu
            .render("design-viewport-menu", |this| &mut this.menu, window, cx);
        div()
            .relative()
            .flex_none()
            .child(
                Theme::of(cx)
                    .button(viewport.to_string(), ButtonStyle::Ghost, None)
                    .id("design-viewport")
                    .w(px(110.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.menu.open(
                            (),
                            Place::At(window.mouse_position()),
                            web::viewport_items(Some(viewport)),
                            |this, path, window, cx| {
                                if let Some(preset) = path
                                    .first()
                                    .and_then(|index| artifact::web::Viewport::PRESETS.get(*index))
                                    .copied()
                                {
                                    let _ = this.owner.update(cx, |view, cx| {
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
                            },
                            window,
                            cx,
                        );
                    })),
            )
            .children(menu)
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
            .absolute()
            .top(px(GUTTER))
            .right(px(GUTTER))
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
