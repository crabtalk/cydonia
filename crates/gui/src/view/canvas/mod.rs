use crate::{
    model::{
        canvas::Canvas,
        workspace::{Entry, Member, Opening},
    },
    view::{
        canvas::actions::Scope,
        kit::menu::MenuHost,
        window::{
            reach::InWindow,
            root::{Cydonia, NewCanvas},
        },
    },
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        App, AppContext as _, Context, Entity, FocusHandle, Focusable, StyleRefinement,
        Subscription, WeakEntity, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        popover::Popup,
        widgets::{Button, Scaffolding as _},
    },
};
use canvas::{
    CanvasEditor, CanvasEvent, CanvasView as Surface, Kinds, Snap, handle,
    kind::Sizing,
    layout,
    model::{self, Node, TextMode},
};
use std::path::PathBuf;

mod actions;
mod chrome;
mod embed;
pub(crate) mod entry_picker;
mod image;
mod sources;
mod transfer;

const CARD_WIDTH: i64 = 360;
const CARD_HEIGHT: i64 = 220;
const GROUP_WIDTH: i64 = 400;
const GROUP_HEIGHT: i64 = 280;
const GROUP_PADDING: i64 = 24;
const TEXT_SIZE: f32 = 24.;
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    Actions,
    Context,
}

pub(crate) struct CanvasView {
    pub(crate) canvas: Entry,
    source: Entity<Canvas>,
    root: WeakEntity<Cydonia>,
    path: PathBuf,
    pub(crate) surface: Entity<Surface>,
    menu: ContextMenu<Self, Menu>,
    at: (i64, i64),
    reference_picker: Popup<entry_picker::ReferencePicker>,
    _events: Option<Subscription>,
    exporting_image: bool,
    chrome: Entity<chrome::Chrome>,
    error: Option<String>,
}

impl Cydonia {
    pub(crate) fn new_canvas_action(
        &mut self,
        _: &NewCanvas,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.workspace.read(cx).active {
            self.new_canvas(project, window, cx);
        }
    }

    pub(crate) fn new_canvas(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_project(project, cx);
        let made = self
            .workspace
            .update(cx, |workspace, cx| workspace.create_canvas(project, cx));
        match made {
            Ok(member) => self.enter_member(member, Opening::Kept, window, cx),
            Err(message) => self.show_error("Cannot create canvas".into(), message, window, cx),
        }
    }

    pub(crate) fn open_canvas(
        &mut self,
        project: usize,
        ix: usize,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let Some(open) = self.workspace.read(cx).projects.get(project) else {
            return;
        };
        let Some(canvas) = open.canvases.get(ix) else {
            return;
        };
        let member = Member::new(open.id, Kind::Canvas, canvas.id.clone());
        self.enter_member(member, opening, window, cx);
    }
}

impl CanvasView {
    pub(crate) fn new(
        canvas: Entry,
        source: Entity<Canvas>,
        root: WeakEntity<Cydonia>,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let document = source.read(cx).document.clone();
        let mut kinds = Kinds::new().with_root(path.clone()).with_fresh(|| Node {
            kind: model::TEXT.into(),
            width: CARD_WIDTH,
            height: 40,
            text_mode: Some(TextMode::Label),
            font_size: Some(TEXT_SIZE),
            text: Some(String::new()),
            ..Node::default()
        });
        for name in [model::TEXT, model::FILE, model::LINK, model::GROUP] {
            let mut declared = kinds.get(name).clone().handles(handle::frame);
            if name == model::TEXT {
                declared = declared.sizing(Sizing::Fixed);
            }
            kinds = kinds.with(name, declared);
        }
        kinds = sources::kinds(kinds, path.clone());
        let surface = cx.new(|cx| {
            Surface::for_document(document, layout::FREE, cx)
                .with_kinds(kinds)
                .with_options(canvas::Options {
                    external_drops: true,
                    ..Default::default()
                })
                .with_snap(Snap {
                    grid: Some(20),
                    guides: false,
                })
        });
        let error = source.read(cx).error.clone();
        cx.observe(&source, |this, source, cx| {
            let error = &source.read(cx).error;
            if &this.error != error {
                this.error = error.clone();
                cx.notify();
            }
        })
        .detach();
        let owner = cx.entity().downgrade();
        let chrome = cx.new(|cx| chrome::Chrome::new(owner, &surface, cx));
        Self {
            canvas,
            source,
            root,
            path,
            surface,
            menu: ContextMenu::default(),
            at: (0, 0),
            reference_picker: Popup::default(),
            _events: None,
            exporting_image: false,
            chrome,
            error,
        }
    }

    fn add_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let node = self.surface.read(cx).editor().kinds().fresh(None);
        let at = (self.at.0 - node.width / 2, self.at.1 - node.height / 2);
        self.surface
            .update(cx, |surface, cx| surface.create_node(node, at, window, cx));
    }

    fn add_text_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.surface.update(cx, |surface, cx| {
            surface.create_node(
                Node {
                    kind: model::TEXT.into(),
                    width: CARD_WIDTH,
                    height: CARD_HEIGHT,
                    text: Some(String::new()),
                    ..Node::default()
                },
                self.at,
                window,
                cx,
            );
        });
    }

    fn add_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.surface.update(cx, |surface, cx| {
            surface.create_node(
                Node {
                    kind: model::LINK.into(),
                    width: CARD_WIDTH,
                    height: 360,
                    ..Node::default()
                },
                self.at,
                window,
                cx,
            );
        });
    }

    fn add_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.surface.update(cx, |surface, cx| {
            surface.create_node(
                Node {
                    kind: model::GROUP.into(),
                    width: GROUP_WIDTH,
                    height: GROUP_HEIGHT,
                    label: Some("Group".into()),
                    ..Node::default()
                },
                (self.at.0 - GROUP_WIDTH / 2, self.at.1 - GROUP_HEIGHT / 2),
                window,
                cx,
            );
        });
    }

    fn group_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.surface.update(cx, |surface, cx| {
            let added =
                surface.update_editor(cx, |editor| editor.group_selected("Group", GROUP_PADDING));
            surface.edit_added(added, window, cx);
        });
    }

    fn navigate(
        &mut self,
        command: fn(&mut CanvasEditor),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.surface
            .update(cx, |surface, cx| surface.update_editor(cx, command));
        window.focus(&self.surface.read(cx).focus_handle(cx), cx);
    }
}

impl InWindow for CanvasView {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl MenuHost for CanvasView {
    type Key = Menu;
    fn menu(&self) -> &ContextMenu<Self, Menu> {
        &self.menu
    }
    fn menu_mut(&mut self) -> &mut ContextMenu<Self, Menu> {
        &mut self.menu
    }
}
impl Focusable for CanvasView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.surface.read(cx).focus_handle(cx)
    }
}
impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._events.is_none() {
            self._events = Some(cx.subscribe_in(
                &self.surface,
                window,
                |this, _, event: &CanvasEvent, window, cx| {
                    if let CanvasEvent::OpenRequested { id } = event {
                        let node = this.surface.read(cx).editor().canvas().node(id).cloned();
                        if let Some(node) = node {
                            this.open_source(&node, window, cx);
                        }
                    }
                    if let CanvasEvent::EditRequested { id } = event {
                        let node = this.surface.read(cx).editor().canvas().node(id);
                        if node.is_some_and(|node| node.kind == model::FILE) {
                            this.pick_files(Some(id.clone()), cx);
                        } else {
                            this.pick_reference(Some(id.clone()), window, cx);
                        }
                    }
                    if let CanvasEvent::FilesDropped { at, paths } = event {
                        this.at = (at.x.round() as i64, at.y.round() as i64);
                        this.add_files(paths.clone(), None, cx);
                    }
                    if let CanvasEvent::ContextMenu { position, at, item } = event {
                        this.at = (at.x.round() as i64, at.y.round() as i64);
                        this.toggle_menu(
                            Menu::Context,
                            Place::At(*position),
                            |this, _, cx| {
                                this.rows(
                                    if item.is_none() {
                                        Scope::Background
                                    } else {
                                        Scope::Selection
                                    },
                                    cx,
                                )
                            },
                            window,
                            cx,
                        );
                    }
                },
            ));
        }
        let theme = Theme::of(cx).clone();
        let error = self.error.clone();
        div()
            .flex_1()
            .min_w_0()
            .size_full()
            .min_h_0()
            .relative()
            .child(self.surface.clone())
            .children(self.reference_picker(cx))
            .child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .left(px(12.))
                    .right(px(12.))
                    .child(
                        self.chrome
                            .clone()
                            .cached(StyleRefinement::default().w_full().h(px(chrome::HEIGHT))),
                    ),
            )
            .when(self.exporting_image, |element| {
                element.child(
                    theme
                        .group_box()
                        .absolute()
                        .bottom(px(60.))
                        .left(px(12.))
                        .p(px(8.))
                        .child(
                            div()
                                .text_style(TextStyle::Callout)
                                .child("Exporting image…"),
                        ),
                )
            })
            .when_some(error, |element, message| {
                element.child(
                    theme
                        .group_box()
                        .absolute()
                        .top(px(12.))
                        .left(px(12.))
                        .right(px(12.))
                        .p(px(12.))
                        .gap(px(8.))
                        .child(
                            div()
                                .text_style(TextStyle::Callout)
                                .text_color(theme.danger)
                                .child(message),
                        )
                        .child(
                            Button::new("canvas-reload", "Reload saved canvas").on_press(
                                cx.listener(|this, _, window, cx| {
                                    this.source.update(cx, |source, cx| source.reload(cx));
                                    window.focus(&this.focus_handle(cx), cx);
                                }),
                            ),
                        )
                        .child(Button::new("canvas-keep", "Save my version").on_press(
                            cx.listener(|this, _, window, cx| {
                                this.source.update(cx, |source, cx| source.keep(cx));
                                window.focus(&this.focus_handle(cx), cx);
                            }),
                        )),
                )
            })
            .children(self.menu_here(&Menu::Context, "canvas-context-menu", window, cx))
    }
}
