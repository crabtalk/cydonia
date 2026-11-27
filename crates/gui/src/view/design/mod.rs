use crate::{
    model::{
        store::Store,
        workspace::{Entry, Workspace},
    },
    view::{
        design::{
            preview::{LiveArtboard, Preview, Revision, THUMBNAIL_WIDTH},
            toolbar::ViewportButton,
        },
        kit::{
            menu::{self, Act, MenuHost},
            rename::{Rename, Renaming},
        },
        window::{
            reach::InWindow,
            root::{Cydonia, NewDesign},
        },
    },
};
use artifact::{
    design::{Artboard, Design},
    error::Error,
    kind::Kind,
    web::Viewport,
};
use bezel::{
    gpui::{
        self, App, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Render, Size,
        Subscription, Task, WeakEntity, Window, actions, div, point, prelude::*, px, size,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::KEY_CONTEXT,
        menu::Item,
    },
};
use browser::{WebView, WebViewEvent};
use canvas::{CanvasView as Surface, Document};
use std::{collections::HashMap, path::Path, time::Duration};

pub(crate) mod preview;
mod surface;
mod toolbar;

const CONTEXT: &str = "DesignArtboard";
const GUTTER: f32 = 16.;
const ZOOM_SETTLE: Duration = Duration::from_millis(150);
/// What one zoom in multiplies an open artboard's magnification by, as the
/// canvas's does its zoom.
const MAGNIFICATION_STEP: f32 = 1.25;
/// The scales the zoom menu offers by name.
const ZOOM_LEVELS: [f32; 6] = [0.25, 0.5, 0.75, 1., 1.5, 2.];
const MAX_MAGNIFICATION: f32 = 8.;
const PAN_EVAL: Duration = Duration::from_secs(1);

actions!(
    design_view,
    [Back, ZoomIn, ZoomOut, FitArtboards, SelectTool, HandTool]
);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let predicate = format!("{CONTEXT} && !{KEY_CONTEXT}");
    let predicate = Some(predicate.as_str());
    let [zoom_in, zoom_in_plus, zoom_out] = if cfg!(target_os = "macos") {
        ["cmd-=", "cmd-+", "cmd--"]
    } else {
        ["ctrl-=", "ctrl-+", "ctrl--"]
    };
    vec![
        KeyBinding::new("escape", Back, predicate),
        KeyBinding::new(zoom_in, ZoomIn, predicate),
        KeyBinding::new(zoom_in_plus, ZoomIn, predicate),
        KeyBinding::new(zoom_out, ZoomOut, predicate),
        KeyBinding::new("shift-1", FitArtboards, predicate),
        KeyBinding::new("v", SelectTool, predicate),
        KeyBinding::new("h", HandTool, predicate),
    ]
}

pub(crate) struct DesignView {
    pub(crate) entry: Entry,
    source: Entity<Design>,
    store: Store,
    root: WeakEntity<Cydonia>,
    focus: FocusHandle,
    open: Option<String>,
    /// The artboard opened last, which the one-artboard view goes back to.
    last_open: Option<String>,
    document: Entity<Document>,
    fitted: bool,
    surface: Entity<Surface>,
    viewport_button: Entity<ViewportButton>,
    hover: Option<(Revision, LiveArtboard, Subscription)>,
    available: Size<gpui::Pixels>,
    thumbnails: HashMap<Revision, Result<Preview, String>>,
    details: HashMap<Revision, Detail>,
    zoom_settle: Option<Task<()>>,
    loading: Option<Capture>,
    live: Option<(Revision, Result<LiveArtboard, String>)>,
    /// Redraws the zoom's reading when a pinch magnifies the open artboard.
    magnified: Option<Subscription>,
    menu: ContextMenu<Self, DesignMenu>,
    rename: Rename,
    tool: Tool,
    /// Space is held: the hand stands in for the tool.
    space: bool,
    /// Where the hand's drag last was.
    pan_from: Option<gpui::Point<gpui::Pixels>>,
    error: Option<String>,
}

/// What a press on the design does.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    /// Opens an artboard, and works the open artboard's page.
    Select,
    /// Pans the canvas, or the open artboard's page.
    Hand,
}

/// Which of the view's menus is up.
#[derive(Clone, PartialEq)]
pub(crate) enum DesignMenu {
    Zoom,
    /// One artboard's, by its id, where it was right-pressed.
    Artboard(String),
}

#[derive(Clone, Copy)]
enum Zoom {
    In,
    Out,
    Fit,
    /// To this scale of the artboard's own pixels.
    To(f32),
}

struct Detail {
    requested_width: u32,
    preview: Option<Preview>,
}

struct Capture {
    key: Revision,
    browser: Entity<WebView>,
    _task: Task<()>,
}

impl Cydonia {
    pub(crate) fn new_design_action(
        &mut self,
        _: &NewDesign,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.workspace.read(cx).active {
            self.new_design(project, window, cx);
        }
    }

    pub(crate) fn new_design(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit(cx) {
            return;
        }
        self.select_project(project, cx);
        let made = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_design(project, cx));
        match made {
            Ok(member) => self.land_member(member, window, cx),
            Err(error) => self.show_error("Cannot create design".into(), error, window, cx),
        }
    }
}

impl DesignView {
    pub(crate) fn new(
        entry: Entry,
        source: Entity<Design>,
        store: Store,
        root: WeakEntity<Cydonia>,
        workspace: Entity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        let (document, surface) = Self::make_surface(cx);
        cx.observe(&surface, |this, _, cx| {
            if this.open.is_some() {
                return;
            }
            let timer = cx.background_executor().timer(ZOOM_SETTLE);
            this.zoom_settle = Some(cx.spawn(async move |this, cx| {
                timer.await;
                let _ = this.update(cx, |this, cx| {
                    this.zoom_settle = None;
                    cx.notify();
                });
            }));
        })
        .detach();
        let owner = cx.entity().downgrade();
        let viewport_button = cx.new(|cx| ViewportButton::new(owner, source.clone(), cx));
        Self {
            entry,
            source,
            store,
            root,
            focus: cx.focus_handle(),
            open: None,
            last_open: None,
            document,
            fitted: false,
            surface,
            viewport_button,
            hover: None,
            available: size(px(0.), px(0.)),
            thumbnails: HashMap::new(),
            details: HashMap::new(),
            zoom_settle: None,
            loading: None,
            live: None,
            magnified: None,
            menu: ContextMenu::default(),
            rename: Rename::new(workspace, |this: &mut Self| &mut this.rename, cx),
            tool: Tool::Select,
            space: false,
            pan_from: None,
            error: None,
        }
    }

    fn expand(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.last_open = Some(id.clone());
        self.open = Some(id);
        self.hover = None;
        self.loading = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Put the name field over artboard `id`'s title.
    fn rename_artboard(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.rename
            .start(Renaming::Artboard(self.entry.clone(), id), window, cx);
        cx.notify();
    }

    /// Artboard `id`'s menu, at the window point `at`.
    fn artboard_menu(
        &mut self,
        id: String,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self
            .source
            .read(cx)
            .artboards
            .iter()
            .find(|artboard| artboard.id == id)
            .map(|artboard| artboard.title.clone())
        else {
            return;
        };
        let key = DesignMenu::Artboard(id.clone());
        let renamed = id.clone();
        let rows: Vec<(Item, Act<Self>)> = vec![
            menu::row(
                Item::action("Rename").with_icon(icons::text::SquarePen),
                move |this: &mut Self, window, cx| {
                    this.rename_artboard(renamed.clone(), window, cx)
                },
            ),
            menu::row(
                Item::action("Delete").with_icon(icons::files::Trash),
                move |this: &mut Self, window, cx| {
                    let (entry, id, title) = (this.entry.clone(), id.clone(), title.clone());
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.ask_delete_artboard(entry, id, title, window, cx)
                    });
                },
            ),
        ];
        self.toggle_menu(key, Place::At(at), |_, _, _| rows, window, cx);
    }

    /// Add an empty artboard after the last.
    fn new_artboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = format!("Artboard {}", self.source.read(cx).artboards.len() + 1);
        self.defer_edit(
            window,
            cx,
            move |workspace, entry, cx| workspace.add_design_artboard(entry, &title, cx),
            |_, _, _| {},
        );
    }

    pub(crate) fn open_browser(&self) -> Option<Entity<WebView>> {
        self.live
            .as_ref()
            .and_then(|(_, live)| live.as_ref().ok())
            .map(|live| live.browser.clone())
    }

    pub(crate) fn live_artboard(
        &self,
        artboard: &str,
        viewport: Viewport,
        cx: &App,
    ) -> Option<LiveArtboard> {
        let (held, live) = self.live.as_ref()?;
        let artboard = self
            .source
            .read(cx)
            .artboards
            .iter()
            .find(|held| held.id == artboard)?;
        (Some(held) == Revision::new(artboard, viewport).as_ref())
            .then_some(live.as_ref().ok()?)
            .cloned()
    }

    /// How large an open artboard's page is drawn against its own pixels
    /// before it is magnified.
    fn fitted_scale(&self, cx: &App) -> f32 {
        let viewport = self.source.read(cx).viewport;
        preview::fit(viewport, self.available).width.as_f32() / viewport.width as f32
    }

    /// The scale the zoom reads: the open artboard's page against its own
    /// pixels, or the canvas's zoom.
    fn scale(&self, cx: &App) -> f32 {
        match self.open_browser() {
            Some(browser) => self.fitted_scale(cx) * browser.read(cx).magnification(),
            None => self.surface.read(cx).editor().zoom(),
        }
    }

    fn active_tool(&self) -> Tool {
        if self.space { Tool::Hand } else { self.tool }
    }

    fn set_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool == tool {
            return;
        }
        self.tool = tool;
        cx.notify();
    }

    /// Moves what is shown with the pointer by `delta`: the open artboard's
    /// page, or the canvas.
    fn pan(&mut self, delta: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(browser) = self.open_browser() else {
            self.surface.update(cx, |surface, cx| {
                surface.update_editor(cx, |editor| {
                    editor.pan_by(delta.x.as_f32(), delta.y.as_f32())
                })
            });
            return;
        };
        let scale = self.scale(cx);
        if scale <= 0. {
            return;
        }
        let script = format!(
            "scrollBy({}, {})",
            -delta.x.as_f32() / scale,
            -delta.y.as_f32() / scale
        );
        browser
            .read(cx)
            .eval::<serde_json::Value>(&script, PAN_EVAL, cx)
            .detach();
    }

    /// Magnifies the open artboard's page, or zooms the canvas. Fitting the
    /// artboards from an open one goes back to the canvas.
    fn zoom(&mut self, zoom: Zoom, window: &mut Window, cx: &mut Context<Self>) {
        if let Zoom::Fit = zoom
            && self.open.is_some()
        {
            self.close_artboard(window, cx);
        }
        let Some(browser) = self.open_browser() else {
            self.surface.update(cx, |surface, cx| {
                surface.update_editor(cx, |editor| match zoom {
                    Zoom::In => editor.zoom_in(),
                    Zoom::Out => editor.zoom_out(),
                    Zoom::Fit => editor.fit(),
                    Zoom::To(scale) => editor.set_zoom(scale),
                })
            });
            return;
        };
        let fitted = self.fitted_scale(cx);
        browser.update(cx, |page, cx| {
            let level = page.magnification();
            let level = match zoom {
                Zoom::In => level * MAGNIFICATION_STEP,
                Zoom::Out => level / MAGNIFICATION_STEP,
                Zoom::Fit => 1.,
                Zoom::To(scale) if fitted > 0. => scale / fitted,
                Zoom::To(_) => level,
            };
            page.set_magnification(level.clamp(1., MAX_MAGNIFICATION));
            cx.notify();
        });
        cx.notify();
    }

    fn close_artboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.take().is_none() {
            cx.propagate();
            return;
        }
        self.park();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn defer_edit<T: 'static>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Workspace, &Entry, &mut Context<Workspace>) -> Result<T, Error> + 'static,
        saved: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    ) {
        let view = cx.weak_entity();
        let entry = self.entry.clone();
        self.defer_window(window, cx, move |root, _, cx| {
            let result = root
                .workspace
                .update(cx, |workspace, cx| edit(workspace, &entry, cx));
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(value) => {
                        this.error = None;
                        saved(this, value, cx);
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        });
    }

    fn advance_preview(
        &mut self,
        record: &Design,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading.is_some() {
            return;
        }
        let artboards = || {
            record.artboards.iter().filter_map(|artboard| {
                Revision::new(artboard, record.viewport).map(|key| (artboard, key))
            })
        };
        if let Some((artboard, key)) =
            artboards().find(|(_, key)| !self.thumbnails.contains_key(key))
        {
            self.capture(artboard, key, record.viewport, None, folder, window, cx);
            return;
        }
        if !self.fitted || self.zoom_settle.is_some() {
            return;
        }

        let editor = self.surface.read(cx).editor();
        let Some((x, y, width, height)) = editor.visible() else {
            return;
        };
        let visible = Bounds::new(point(x, y), size(width, height));
        let zoom = editor.zoom();
        let scale_factor = window.scale_factor();
        let visible_artboard = |id: &str| {
            editor.painted().node(id).is_some_and(|node| {
                let bounds = Bounds::new(
                    point(node.x as f32, node.y as f32),
                    size(node.width as f32, node.height as f32),
                );
                !visible.intersect(&bounds).is_empty()
            })
        };
        self.details.retain(|key, detail| {
            if visible_artboard(&key.artboard) {
                return true;
            }
            if let Some(preview) = detail.preview.take() {
                let _ = window.drop_image(preview.image);
            }
            false
        });
        let candidate = artboards().find_map(|(artboard, key)| {
            let thumbnail = self.thumbnails.get(&key)?.as_ref().ok()?;
            let width = thumbnail.detail_width(record.viewport, zoom, scale_factor);
            (width > thumbnail.image.size(0).width.0 as u32
                && visible_artboard(&artboard.id)
                && self
                    .details
                    .get(&key)
                    .is_none_or(|detail| width > detail.requested_width))
            .then_some((artboard, key, width))
        });
        if let Some((artboard, key, width)) = candidate {
            self.capture(
                artboard,
                key,
                record.viewport,
                Some(width),
                folder,
                window,
                cx,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn capture(
        &mut self,
        artboard: &Artboard,
        key: Revision,
        viewport: Viewport,
        detail: Option<u32>,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match preview::open(folder, artboard, viewport, false, window, cx) {
            Ok(live) => {
                let browser = live.browser.clone();
                let captured = key.clone();
                let task = cx.spawn(async move |this, cx| {
                    let width = detail.unwrap_or_else(|| viewport.width.min(THUMBNAIL_WIDTH));
                    let image = live.preview(viewport, width, cx).await;
                    let _ = this.update(cx, |this, cx| {
                        if this.loading.as_ref().is_some_and(|job| job.key == captured) {
                            this.loading = None;
                            this.preview_captured(captured, detail, image, cx);
                            cx.notify();
                        }
                    });
                });
                self.loading = Some(Capture {
                    key,
                    browser,
                    _task: task,
                });
            }
            Err(error) => {
                self.preview_captured(key, detail, Err(error), cx);
                cx.notify();
            }
        }
    }

    fn preview_captured(
        &mut self,
        key: Revision,
        detail: Option<u32>,
        image: Result<Preview, String>,
        cx: &mut App,
    ) {
        if let Some(requested_width) = detail {
            let detail = self.details.entry(key).or_insert(Detail {
                requested_width,
                preview: None,
            });
            detail.requested_width = requested_width;
            if let Ok(image) = image
                && detail
                    .preview
                    .as_ref()
                    .is_none_or(|old| image.image.size(0).width > old.image.size(0).width)
                && let Some(old) = detail.preview.replace(image)
            {
                cx.drop_image(old.image, None);
            }
        } else {
            self.thumbnails.insert(key, image);
        }
    }

    pub(crate) fn park(&mut self) {
        self.live = None;
        self.hover = None;
        self.loading = None;
    }
}

impl MenuHost for DesignView {
    type Key = DesignMenu;

    fn menu(&self) -> &ContextMenu<Self, DesignMenu> {
        &self.menu
    }

    fn menu_mut(&mut self) -> &mut ContextMenu<Self, DesignMenu> {
        &mut self.menu
    }
}

impl InWindow for DesignView {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl Focusable for DesignView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DesignView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.source.read(cx).revisions_loaded {
            let entry = self.entry.clone();
            self.defer_window(window, cx, move |root, _, cx| {
                root.workspace
                    .update(cx, |workspace, cx| workspace.load_design(&entry, cx));
            });
        }
        let record = self.source.read(cx).clone();
        let theme = Theme::of(cx).clone();
        let folder = self.store.place(Kind::Design, &record.id);
        let shown_key = self
            .open
            .as_ref()
            .and_then(|id| record.artboards.iter().find(|artboard| &artboard.id == id));
        let revisions: Vec<_> = record
            .artboards
            .iter()
            .filter_map(|artboard| Revision::new(artboard, record.viewport))
            .collect();
        self.thumbnails.retain(|key, _| revisions.contains(key));
        self.details.retain(|key, detail| {
            if revisions.contains(key) {
                return true;
            }
            if let Some(preview) = detail.preview.take() {
                let _ = window.drop_image(preview.image);
            }
            false
        });
        if self
            .loading
            .as_ref()
            .is_some_and(|job| !revisions.contains(&job.key))
        {
            self.loading = None;
        }
        if self.open.is_some() && shown_key.is_none() {
            self.open = None;
            self.live = None;
        }
        let owner = cx.entity().downgrade();
        let measure = gpui::canvas(
            |bounds, _, _| bounds.size,
            move |_, size, window, cx| {
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        if this.available != size {
                            this.available = size;
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .inset_0();
        let body = if let Some(artboard) = shown_key {
            let key = Revision::new(artboard, record.viewport);
            if let Some(key) = key
                && self.live.as_ref().is_none_or(|(held, _)| held != &key)
            {
                let opened = folder
                    .as_deref()
                    .ok_or_else(|| "Design directory unavailable".into())
                    .and_then(|folder| {
                        preview::open(folder, artboard, record.viewport, true, window, cx)
                    });
                self.magnified = opened.as_ref().ok().map(|live| {
                    cx.subscribe(&live.browser, |_, _, event: &WebViewEvent, cx| {
                        if let WebViewEvent::Magnified = event {
                            cx.notify();
                        }
                    })
                });
                self.live = Some((key, opened));
            }
            if artboard.revision.is_none() {
                self.live = None;
            }
            match self.live.as_ref().map(|(_, live)| live) {
                Some(Ok(live)) => {
                    let browser = live.browser.clone();
                    let fit = preview::fit(record.viewport, self.available);
                    let review = div().w(fit.width).h(fit.height).child(browser);
                    div()
                        .size_full()
                        .bg(theme.canvas_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(review)
                        .into_any_element()
                }
                preview => div()
                    .p(px(GUTTER))
                    .text_style(TextStyle::Callout)
                    .child(match preview {
                        Some(Err(error)) => error.clone(),
                        _ => "Waiting for index.html".into(),
                    })
                    .into_any_element(),
            }
        } else {
            if let Some(folder) = &folder {
                self.advance_preview(&record, folder, window, cx);
            }
            self.canvas(&record, cx)
        };
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &Back, window, cx| this.close_artboard(window, cx)))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.zoom(Zoom::In, window, cx)))
            .on_action(
                cx.listener(|this, _: &ZoomOut, window, cx| this.zoom(Zoom::Out, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FitArtboards, window, cx| this.zoom(Zoom::Fit, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectTool, _, cx| this.set_tool(Tool::Select, cx)))
            .on_action(cx.listener(|this, _: &HandTool, _, cx| this.set_tool(Tool::Hand, cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "space" && !this.space && this.focus.is_focused(window) {
                    this.space = true;
                    cx.notify();
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, cx| {
                if event.keystroke.key == "space" && this.space {
                    this.space = false;
                    this.pan_from = None;
                    cx.notify();
                }
            }))
            .children(self.error.as_ref().map(|error| {
                div()
                    .px(px(GUTTER))
                    .text_color(theme.danger)
                    .child(error.clone())
            }))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(measure)
                    .child(body)
                    .children((self.active_tool() == Tool::Hand).then(|| self.hand(cx)))
                    .child(self.view_switch(cx))
                    .child(self.dock(cx))
                    .child(self.corner(window, cx))
                    .children(
                        self.menu()
                            .key()
                            .filter(|key| matches!(key, DesignMenu::Artboard(_)))
                            .cloned()
                            .and_then(|key| {
                                self.menu_here(&key, "design-artboard-menu", window, cx)
                            }),
                    )
                    .children(
                        self.loading
                            .as_ref()
                            .map(|job| preview::host(job.browser.clone())),
                    ),
            )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/design_bindings.rs"]
mod tests;
