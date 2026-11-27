use crate::{
    model::{file::File, workspace::references},
    view::{
        canvas::CanvasView,
        kit::web::{
            Navigation, WebPresentation, console::ConsoleLog, frame_header, viewport_items,
        },
    },
};
use artifact::{
    canvas::web::{self, Frame, Source},
    web::Viewport,
};
use bezel::{
    gpui::{
        self, AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _,
        ObjectFit, Render, StyledImage as _, Subscription, Task, WeakEntity, Window, div, img,
        point, prelude::*, px, size,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use browser::{WebView, WebViewEvent};
use canvas::{
    Change,
    kind::{self, Card, Chrome},
    model::Node,
};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

#[derive(Clone)]
pub(super) struct Frames {
    views: Rc<RefCell<HashMap<String, Entity<Page>>>>,
    owner: WeakEntity<CanvasView>,
    root: PathBuf,
}

impl Frames {
    pub(super) fn new(owner: WeakEntity<CanvasView>, root: PathBuf) -> Self {
        Self {
            views: Rc::default(),
            owner,
            root,
        }
    }

    pub(super) fn retain(&self, document: &canvas::model::Canvas) {
        self.views
            .borrow_mut()
            .retain(|id, _| document.node(id).is_some_and(|node| node.kind == web::KIND));
    }

    pub(super) fn kind(&self) -> kind::Kind {
        let frames = self.clone();
        let exports = self.clone();
        kind::Kind::new(|node, look, _, cx| {
            kind::chrome(Chrome::Outline, node, look.card.zoom, cx)
                .children(look.editor)
                .into_any_element()
        })
        .controls(move |node, card, window, cx| Some(frames.render(node, card, window, cx)))
        .edit(
            kind::Field::new(
                |node| {
                    node.url
                        .clone()
                        .or_else(|| node.file.clone())
                        .unwrap_or_default()
                },
                |node, value| {
                    node.url = Some(value);
                    node.file = None;
                },
            )
            .with_control(|node| {
                if node.file.is_some() {
                    kind::Control::External
                } else {
                    kind::Control::Url
                }
            }),
        )
        .with_export(move |node, cx| exports.snapshot(node, cx))
        .retained()
        .handles(canvas::handle::frame)
    }

    fn page(&self, node: &Node, cx: &App) -> Result<Entity<Page>, String> {
        let page = self
            .views
            .borrow()
            .get(&node.id)
            .cloned()
            .ok_or("web frame has not rendered yet")?;
        if Frame::try_from(node) != page.read(cx).frame {
            return Err("web frame source or viewport has changed; waiting for its preview".into());
        }
        Ok(page)
    }

    fn snapshot(&self, node: &Node, cx: &mut App) -> Task<Result<kind::Paint, String>> {
        let page = match self.page(node, cx) {
            Ok(page) => page,
            Err(error) => return Task::ready(Err(error)),
        };
        let page = page.read(cx);
        let browser = match page.webview() {
            Ok(browser) => browser,
            Err(error) => return Task::ready(Err(error)),
        };
        if !page.navigation.ready(browser.read(cx)) {
            return Task::ready(Err("web frame is still loading".into()));
        }
        let title = Page::title(node, &page.frame);
        let snapshot = browser.read(cx).snapshot(cx);
        cx.foreground_executor().spawn(async move {
            let image = snapshot.await?;
            let paint: kind::Paint = Rc::new(move |node, look, _, cx| {
                kind::chrome(Chrome::Outline, node, look.card.zoom, cx)
                    .child(
                        div()
                            .size_full()
                            .relative()
                            .bg(Theme::of(cx).surface_card)
                            .child(
                                img(image.clone())
                                    .size_full()
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(frame_header(title.clone(), cx)),
                    )
                    .into_any_element()
            });
            Ok(paint)
        })
    }

    fn render(&self, node: &Node, card: &Card, window: &mut Window, cx: &mut App) -> AnyElement {
        let page = {
            let mut views = self.views.borrow_mut();
            views
                .entry(node.id.clone())
                .or_insert_with(|| {
                    cx.new(|cx| {
                        Page::new(
                            node.clone(),
                            card.clone(),
                            self.root.clone(),
                            self.owner.clone(),
                            window,
                            cx,
                        )
                    })
                })
                .clone()
        };
        page.update(cx, |page, cx| page.configure(node, card, window, cx));
        div().absolute().inset_0().child(page).into_any_element()
    }
}

pub(crate) struct Page {
    pub(crate) console: ConsoleLog,
    node: Node,
    card: Card,
    root: PathBuf,
    owner: WeakEntity<CanvasView>,
    frame: Result<Frame, String>,
    browser: Option<Entity<WebView>>,
    interactive: bool,
    navigation: Navigation,
    focus: FocusHandle,
    source: Option<Entity<File>>,
    _file: Option<Subscription>,
    _page: Option<Subscription>,
    menu: ContextMenu<Self>,
    source_error: Option<String>,
    presentation_error: Option<String>,
}

impl Page {
    pub(crate) fn webview(&self) -> Result<Entity<WebView>, String> {
        if let Some(error) = self
            .source_error
            .as_ref()
            .or(self.presentation_error.as_ref())
        {
            return Err(error.clone());
        }
        self.browser
            .clone()
            .ok_or_else(|| "web frame is unavailable".into())
    }

    pub(crate) fn ready(&self, cx: &App) -> bool {
        self.browser
            .as_ref()
            .is_some_and(|browser| self.navigation.ready(browser.read(cx)))
    }

    fn new(
        node: Node,
        card: Card,
        root: PathBuf,
        owner: WeakEntity<CanvasView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut page = Self {
            frame: Frame::try_from(&node),
            node,
            card,
            root,
            owner,
            browser: None,
            console: ConsoleLog::default(),
            interactive: false,
            navigation: Navigation::default(),
            focus: cx.focus_handle(),
            source: None,
            _file: None,
            _page: None,
            menu: ContextMenu::default(),
            source_error: None,
            presentation_error: None,
        };
        page.open(window, cx);
        page
    }

    fn title(node: &Node, frame: &Result<Frame, String>) -> String {
        node.label
            .clone()
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| match frame.as_ref().map(|frame| &frame.source) {
                Ok(Source::File(file)) => file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                Ok(Source::Url(url)) => url.host_str().unwrap_or("Web page").to_owned(),
                Err(_) => "Web page".into(),
            })
    }

    fn configure(&mut self, node: &Node, card: &Card, window: &mut Window, cx: &mut Context<Self>) {
        let frame = Frame::try_from(node);
        let changed = frame.as_ref().map(|frame| &frame.source)
            != self.frame.as_ref().map(|frame| &frame.source);
        if &self.node != node || self.card.size != card.size || self.card.zoom != card.zoom {
            self.node = node.clone();
            self.card = card.clone();
            self.frame = frame;
            if changed {
                self.open(window, cx);
            }
            self.present(window, cx);
            cx.notify();
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigation.reset();
        self._file = None;
        self.source = None;
        self.source_error = None;
        let resolved = self
            .frame
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|frame| frame.source.resolve(&self.root));
        let resolved = match resolved {
            Ok(resolved) => resolved,
            Err(error) => {
                self.browser = None;
                self._page = None;
                self.source_error = Some(error);
                return;
            }
        };
        if let Some(page) = &self.browser {
            page.update(cx, |page, _| page.load(resolved.url));
        } else {
            let root = self.root.clone();
            let page = cx.new(|cx| {
                WebView::new(resolved.url, window, cx)
                    .with_read_access(root)
                    .with_console()
            });
            self._page = Some(cx.subscribe(
                &page,
                |this, page, event: &WebViewEvent, cx| match event {
                    WebViewEvent::Console(message) => this.console.push(message.clone()),
                    WebViewEvent::Load(state) => {
                        this.navigation.update(*state, page.read(cx));
                        cx.notify();
                    }
                    WebViewEvent::Error(_) => cx.notify(),
                    _ => {}
                },
            ));
            self.browser = Some(page);
        }
        if let Some(path) = resolved.file
            && let Some(workspace) = references::current(cx)
        {
            let file = File::open(path, workspace.downgrade(), cx);
            self._file = Some(cx.observe(&file, |this, file, cx| {
                match &file.read(cx).result {
                    Some(Ok(_)) => {
                        this.source_error = None;
                        if let Some(page) = &this.browser {
                            this.navigation.reset();
                            page.update(cx, |page, _| page.reload_bypassing_cache());
                        }
                    }
                    Some(Err(error)) => {
                        this.source_error = Some(error.clone());
                    }
                    None => {}
                }
                cx.notify();
            }));
            self.source_error = file
                .read(cx)
                .result
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .cloned();
            self.source = Some(file);
        }
        self.present(window, cx);
    }

    fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(page), Ok(frame)) = (&self.browser, &self.frame) else {
            return;
        };
        self.presentation_error = page
            .present(frame.viewport, self.interactive, window, cx)
            .err();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.interactive = !self.interactive;
        self.present(window, cx);
        if self.interactive {
            if let Some(page) = &self.browser {
                window.focus(&page.focus_handle(cx), cx);
            }
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn viewport_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.frame.as_ref().map(|frame| frame.viewport).ok();
        self.menu.open(
            (),
            Place::At(window.mouse_position()),
            viewport_items(current),
            |this, path, _, cx| {
                let Some(viewport) = path
                    .first()
                    .and_then(|index| Viewport::PRESETS.get(*index))
                    .copied()
                else {
                    return;
                };
                let owner = this.owner.clone();
                let id = this.node.id.clone();
                cx.defer(move |cx| {
                    let _ = owner.update(cx, |owner, cx| owner.set_web_viewport(&id, viewport, cx));
                });
            },
            window,
            cx,
        );
    }
}

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let loading = self
            .browser
            .as_ref()
            .is_some_and(|page| page.read(cx).is_loading());
        let viewport = self
            .frame
            .as_ref()
            .map(|frame| frame.viewport)
            .unwrap_or_default();
        let title = Self::title(&self.node, &self.frame);
        let scale = (self.card.size.width.as_f32() / viewport.width as f32)
            .min(self.card.size.height.as_f32() / viewport.height as f32)
            .max(0.);
        let body = if let Some(error) = self
            .source_error
            .as_ref()
            .or(self.presentation_error.as_ref())
        {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(12.))
                .text_style(TextStyle::Callout)
                .text_color(theme.text_faint)
                .child(error.clone())
                .into_any_element()
        } else if let Some(page) = &self.browser {
            div()
                .w(px(viewport.width as f32 * scale))
                .h(px(viewport.height as f32 * scale))
                .child(page.clone())
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let menu = self
            .menu
            .render("web-viewport-menu", |this| &mut this.menu, window, cx);
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .bg(theme.surface_card)
                    .child(body),
            )
            .child(
                frame_header(title.clone(), cx)
                    .child(
                        theme
                            .button(viewport.to_string(), ButtonStyle::Ghost, None)
                            .id("web-viewport")
                            .flex_none()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.viewport_menu(window, cx)),
                            ),
                    )
                    .child(
                        theme
                            .icon_button(icons::arrows::RefreshCw, ButtonStyle::Ghost, None)
                            .id("web-refresh")
                            .flex_none()
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(page) = &this.browser {
                                    this.navigation.reset();
                                    page.update(cx, |page, _| page.reload_bypassing_cache());
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        theme
                            .button(
                                if self.interactive {
                                    "Done"
                                } else if loading {
                                    "Loading…"
                                } else {
                                    "Interact"
                                },
                                ButtonStyle::Ghost,
                                None,
                            )
                            .id("web-interact")
                            .w(px(76.))
                            .flex_none()
                            .when(self.interactive || !loading, |button| {
                                button.on_click(
                                    cx.listener(|this, _, window, cx| this.toggle(window, cx)),
                                )
                            }),
                    ),
            )
            .children(menu)
    }
}

impl CanvasView {
    pub(crate) fn web_frame(&self, id: &str, cx: &App) -> Result<Entity<Page>, String> {
        let surface = self.surface.read(cx);
        let editor = surface.editor();
        let node = editor
            .painted()
            .node(id)
            .filter(|node| node.kind == web::KIND)
            .ok_or_else(|| format!("no web frame {id} on this canvas"))?;
        let in_view = editor.visible().is_some_and(|(x, y, width, height)| {
            let visible = gpui::Bounds::new(point(x, y), size(width, height));
            let frame = gpui::Bounds::new(
                point(node.x as f32, node.y as f32),
                size(node.width as f32, node.height as f32),
            );
            !frame.intersect(&visible).is_empty()
        });
        if !in_view {
            return Err("web frame is outside the canvas viewport; bring it into view".into());
        }
        self.pages.page(node, cx)
    }

    fn set_web_viewport(&mut self, id: &str, viewport: Viewport, cx: &mut Context<Self>) {
        if self.surface.read(cx).editor().options().read_only {
            return;
        }
        self.surface.update(cx, |surface, cx| {
            surface.update_editor(cx, |editor| {
                let Some(mut node) = editor.canvas().node(id).cloned() else {
                    return;
                };
                node.extra.insert(
                    web::VIEWPORT.into(),
                    serde_json::json!({"width":viewport.width,"height":viewport.height}),
                );
                editor.apply([Change::UpdateNode { node }]);
            })
        });
    }
}
