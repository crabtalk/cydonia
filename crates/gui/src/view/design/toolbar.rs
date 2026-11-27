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
        Context, CursorStyle, Div, Entity, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, Render, ScrollEvent, WeakEntity, Window, div, prelude::*, px,
    },
    theme::{Glass, SurfaceStyle, Theme},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        surface,
        tooltip::Tooltip,
        widgets::Controls as _,
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
    /// The top left: all the artboards on the canvas, or one open.
    pub(super) fn view_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        div()
            .absolute()
            .top(px(GUTTER))
            .left(px(GUTTER))
            .child(surface::of(
                "design-view-switch",
                SurfaceStyle::Glass(Glass::Regular),
                move |theme, _, cx| view.update(cx, |this, cx| this.view_switch_row(theme, cx)),
            ))
    }

    fn view_switch_row(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let artboards = &self.source.read(cx).artboards;
        let single = artboards
            .iter()
            .find(|artboard| Some(&artboard.id) == self.last_open.as_ref())
            .or_else(|| artboards.first())
            .map(|artboard| artboard.id.clone());
        let open = self.open.is_some();
        let all = theme
            .toggle_group_icon(icons::glyph::LayoutGrid, !open)
            .id("design-view-all")
            .tooltip(|window, cx| Tooltip::with_keystroke("All artboards", "shift-1", window, cx))
            .on_click(cx.listener(|this, _, window, cx| this.close_artboard(window, cx)));
        let one = theme
            .toggle_group_icon(icons::glyph::Square, open)
            .id("design-view-one")
            .tooltip(|window, cx| Tooltip::text("One artboard", window, cx))
            .when_some(single.filter(|_| !open), |button, id| {
                button.on_click(
                    cx.listener(move |this, _, window, cx| this.expand(id.clone(), window, cx)),
                )
            });
        tool_row().child(all).child(one)
    }

    /// The tools, floating at the bottom middle.
    pub(super) fn dock(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
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
                move |theme, _, cx| view.update(cx, |this, cx| this.tools(theme, cx)),
            ))
    }

    fn tools(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
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
        ]
        .map(|(tool, id, label, key, icon)| {
            theme
                .toggle_group_icon(icon, tool == active)
                .id(id)
                .tooltip(move |window, cx| Tooltip::with_keystroke(label, key, window, cx))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
        });
        tool_row().children(tools)
    }

    /// The top right: the viewport, then the zoom, read as a percentage, that
    /// opens the zoom menu.
    pub(super) fn corner(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
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
                        move |theme, _, cx| {
                            view.update(cx, |this, cx| {
                                let zoom = theme
                                    .select_trigger_with(None::<Div>, percent(this.scale(cx)))
                                    .id("design-zoom")
                                    .rounded(px(Theme::surface_radius()))
                                    .border_0()
                                    .py(px(0.))
                                    .h(px(Theme::BUTTON_HEIGHT + 4.))
                                    .w(px(76.))
                                    .tooltip(|window, cx| Tooltip::text("Zoom", window, cx))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_zoom_menu(window, cx)
                                    }));
                                ContextMenu::trigger(zoom, |this| &mut this.zoom_menu, (), cx)
                            })
                        },
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

/// The padded row a glass tool group lays its buttons in.
fn tool_row() -> Div {
    div()
        .rounded(px(Theme::surface_radius()))
        .flex()
        .flex_row()
        .items_center()
        .p(px(TOOL_PADDING + 1.))
        .gap(px(2.))
}

/// A scale as the zoom reads it.
fn percent(scale: f32) -> String {
    format!("{}%", (scale * 100.).round())
}
