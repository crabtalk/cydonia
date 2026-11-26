use crate::view::{
    design::{CONTEXT, DesignView, FitArtboards, GUTTER, Tool, ZOOM_LEVELS, Zoom, ZoomIn, ZoomOut},
    kit::{
        canvas::TOOL_PADDING,
        web::{self, ViewportLook as _},
    },
};
use artifact::{design::Design, web::Viewport};
use bezel::{
    gpui::{
        App, Context, CursorStyle, Entity, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, Render, ScrollEvent, WeakEntity, Window, div, prelude::*, px,
    },
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
    /// The tools, floating at the bottom middle.
    pub(super) fn dock(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let active = self.active_tool();
        let tools = [
            (
                Tool::Select,
                "design-tool-select",
                "Select",
                "v",
                icons::glyph::MousePointer2,
            ),
            (
                Tool::Hand,
                "design-tool-hand",
                "Hand",
                "h",
                icons::glyph::Hand,
            ),
            (
                Tool::Comment,
                "design-tool-comment",
                "Comment",
                "c",
                icons::glyph::MessageCircle,
            ),
        ]
        .map(|(tool, id, label, key, icon)| {
            theme
                .toggle_group_icon(icon, tool == active)
                .id(id)
                .tooltip(move |window, cx| Tooltip::with_keystroke(label, key, window, cx))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
        });
        let row = div()
            .rounded(px(Theme::surface_radius()))
            .flex()
            .flex_row()
            .items_center()
            .p(px(TOOL_PADDING + 1.))
            .gap(px(2.))
            .children(tools);
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(12.))
            .flex()
            .justify_center()
            .child(surface::of(
                "design-tools",
                SurfaceStyle::Glass(Glass::Regular),
                move |_, _, _| row,
            ))
    }

    /// The top right: the viewport, then the zoom, read as a percentage, that
    /// opens the zoom menu.
    pub(super) fn corner(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let zoom = theme
            .select_trigger_with(None::<bezel::gpui::Div>, percent(self.scale(cx)))
            .id("design-zoom")
            .rounded(px(Theme::surface_radius()))
            .border_0()
            .py(px(0.))
            .h(px(Theme::BUTTON_HEIGHT + 4.))
            .w(px(76.))
            .tooltip(|window, cx| Tooltip::text("Zoom", window, cx))
            .on_click(cx.listener(|this, _, window, cx| this.toggle_zoom_menu(window, cx)));
        let zoom = ContextMenu::trigger(zoom, |this| &mut this.zoom_menu, (), cx);
        div()
            .absolute()
            .top(px(GUTTER))
            .right(px(GUTTER))
            .flex()
            .items_start()
            .gap(px(8.))
            .child(self.viewport_button.clone())
            .child(
                div()
                    .relative()
                    .child(surface::of(
                        "design-zoom-glass",
                        SurfaceStyle::Glass(Glass::Regular),
                        move |_, _, _| zoom,
                    ))
                    .children(self.zoom_menu.render(
                        "design-zoom-menu",
                        |this| &mut this.zoom_menu,
                        window,
                        cx,
                    )),
            )
    }

    /// Over the design while the hand is the tool: a drag or a wheel pans,
    /// and nothing under it takes the press.
    pub(super) fn hand(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let grabbing = self.pan_from.is_some();
        div()
            .id("design-hand")
            .absolute()
            .inset_0()
            .occlude()
            .map(|el| match grabbing {
                true => el.cursor(CursorStyle::ClosedHand),
                false => el.cursor(CursorStyle::OpenHand),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.pan_from = Some(event.position);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some(from) = this.pan_from else { return };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.pan_from = None;
                    cx.notify();
                    return;
                }
                this.pan_from = Some(event.position);
                this.pan(event.position - from, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.pan_from = None;
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                this.pan(event.delta.pixel_delta(window.line_height()), cx);
            }))
    }

    fn toggle_zoom_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        ContextMenu::toggle(
            self,
            |this| &mut this.zoom_menu,
            (),
            Place::BelowEnd,
            |this, window, cx| {
                let scale = this.scale(cx);
                let mut items = vec![
                    Item::action("Zoom in").with_shortcut_in(&ZoomIn, CONTEXT, window, cx),
                    Item::action("Zoom out").with_shortcut_in(&ZoomOut, CONTEXT, window, cx),
                    Item::action("Fit artboards").with_shortcut_in(
                        &FitArtboards,
                        CONTEXT,
                        window,
                        cx,
                    ),
                    Item::Separator,
                    Item::Heading(format!("Now {}", percent(scale)).into()),
                ];
                items.extend(ZOOM_LEVELS.map(|level| Item::action(percent(level))));
                (items, Self::choose_zoom)
            },
            window,
            cx,
        );
    }

    fn choose_zoom(&mut self, path: &[usize], window: &mut Window, cx: &mut Context<Self>) {
        let zoom = match path.first() {
            Some(0) => Zoom::In,
            Some(1) => Zoom::Out,
            Some(2) => Zoom::Fit,
            Some(index) => match index.checked_sub(5).and_then(|at| ZOOM_LEVELS.get(at)) {
                Some(level) => Zoom::To(*level),
                None => return,
            },
            None => return,
        };
        self.zoom(zoom, window, cx);
    }
}

/// A scale as the zoom reads it.
fn percent(scale: f32) -> String {
    format!("{}%", (scale * 100.).round())
}
