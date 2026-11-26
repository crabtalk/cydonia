use crate::view::{
    design::{CONTEXT, DesignView, ZOOM_LEVELS, Zoom, ZoomIn, ZoomOut, ZoomToFit},
    kit::{
        canvas::TOOL_PADDING,
        web::{self, ViewportLook as _},
    },
};
use artifact::{
    design::{Artboard, Design},
    web::Viewport,
};
use bezel::{
    gpui::{App, Context, Entity, Render, WeakEntity, Window, div, prelude::*, px},
    theme::{Glass, SurfaceStyle, Theme},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        surface,
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
                    let icon = icons::icon(viewport.icon())
                        .size(px(14.))
                        .text_color(theme.text_muted);
                    let button = theme
                        .select_trigger_with(Some(icon), viewport.title())
                        .id("design-viewport")
                        .rounded(px(Theme::surface_radius()))
                        .border_0()
                        .py(px(0.))
                        .h(px(Theme::BUTTON_HEIGHT + 4.))
                        .w(px(124.))
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

    /// The toolbar floating at the bottom middle: an open artboard's tools,
    /// then the zoom, read as a percentage, that opens the zoom menu.
    pub(super) fn dock(
        &mut self,
        artboard: Option<&Artboard>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let tools = artboard.map(|artboard| {
            let comment = if self.pins.picking {
                icons::glyph::X
            } else {
                icons::glyph::MessageCirclePlus
            };
            [
                theme
                    .icon_button(comment, ButtonStyle::Ghost, None)
                    .id("artboard-pick")
                    .tooltip(|window, cx| Tooltip::text("Comment on an element", window, cx))
                    .when(self.open_browser().is_some(), |button| {
                        button.on_click(
                            cx.listener(|this, _, window, cx| this.toggle_picking(window, cx)),
                        )
                    })
                    .into_any_element(),
                Self::comments_button(artboard.comments.len(), cx)
                    .on_click(cx.listener(|this, _, _, cx| this.open_comments(cx)))
                    .into_any_element(),
                theme
                    .icon_button(icons::arrows::Minimize2, ButtonStyle::Ghost, None)
                    .id("artboard-collapse")
                    .tooltip(|window, cx| Tooltip::text("Back to canvas", window, cx))
                    .on_click(cx.listener(|this, _, window, cx| this.close_artboard(window, cx)))
                    .into_any_element(),
                div()
                    .w(px(1.))
                    .h(px(16.))
                    .mx(px(2.))
                    .bg(theme.border)
                    .into_any_element(),
            ]
        });
        let percent = format!("{}%", (self.scale(cx) * 100.).round());
        let zoom = theme
            .button(percent, ButtonStyle::Ghost, None)
            .id("design-zoom")
            .w(px(64.))
            .tooltip(|window, cx| Tooltip::text("Zoom", window, cx))
            .on_click(cx.listener(|this, _, window, cx| this.toggle_zoom_menu(window, cx)));
        let zoom = ContextMenu::trigger(zoom, |this| &mut this.zoom_menu, (), cx);
        let row = div()
            .rounded(px(Theme::surface_radius()))
            .flex()
            .flex_row()
            .items_center()
            .p(px(TOOL_PADDING + 1.))
            .gap(px(2.))
            .children(tools.into_iter().flatten())
            .child(zoom);
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(12.))
            .flex()
            .justify_center()
            .child(
                div()
                    .relative()
                    .child(surface::of(
                        "design-dock",
                        SurfaceStyle::Glass(Glass::Regular),
                        move |_, _, _| row,
                    ))
                    .children(self.zoom_menu.render(
                        "design-zoom-menu",
                        |this| &mut this.zoom_menu,
                        window,
                        cx,
                    )),
            )
    }

    fn toggle_zoom_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        ContextMenu::toggle(
            self,
            |this| &mut this.zoom_menu,
            (),
            Place::AboveEnd,
            |this, window, cx| {
                let scale = this.scale(cx);
                let mut items = vec![
                    Item::action("Zoom in").with_shortcut_in(&ZoomIn, CONTEXT, window, cx),
                    Item::action("Zoom out").with_shortcut_in(&ZoomOut, CONTEXT, window, cx),
                    Item::action("Zoom to fit").with_shortcut_in(&ZoomToFit, CONTEXT, window, cx),
                    Item::Separator,
                ];
                items.extend(ZOOM_LEVELS.map(|level| {
                    Item::action(format!("{}%", level * 100.))
                        .checked((scale - level).abs() < 0.005)
                }));
                (items, Self::choose_zoom)
            },
            window,
            cx,
        );
    }

    fn choose_zoom(&mut self, path: &[usize], _: &mut Window, cx: &mut Context<Self>) {
        let zoom = match path.first() {
            Some(0) => Zoom::In,
            Some(1) => Zoom::Out,
            Some(2) => Zoom::Fit,
            Some(index) => match index.checked_sub(4).and_then(|at| ZOOM_LEVELS.get(at)) {
                Some(level) => Zoom::To(*level),
                None => return,
            },
            None => return,
        };
        self.zoom(zoom, cx);
    }
}
