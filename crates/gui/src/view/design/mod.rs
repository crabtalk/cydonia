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
    ui::input::KEY_CONTEXT,
};
use browser::WebView;
use canvas::{CanvasView as Surface, Document};
use std::{collections::HashMap, path::Path, time::Duration};

mod comments;
mod pins;
pub(crate) mod preview;
mod surface;
mod toolbar;

const CONTEXT: &str = "DesignArtboard";
const GUTTER: f32 = 16.;
const ZOOM_SETTLE: Duration = Duration::from_millis(150);

actions!(design_view, [Back]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let predicate = format!("{CONTEXT} && !{KEY_CONTEXT}");
    vec![KeyBinding::new("escape", Back, Some(&predicate))]
}

pub(crate) struct DesignView {
    pub(crate) entry: Entry,
    source: Entity<Design>,
    store: Store,
    root: WeakEntity<Cydonia>,
    focus: FocusHandle,
    open: Option<String>,
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
    error: Option<String>,
    pins: pins::Pins,
    comments: comments::Comments,
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
            error: None,
            pins: pins::Pins::default(),
            comments: comments::Comments::new(cx),
        }
    }

    fn expand(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open = Some(id);
        self.hover = None;
        self.loading = None;
        window.focus(&self.focus, cx);
        cx.notify();
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

    fn comment_base(&self, id: &str) -> Option<String> {
        let folder = self.store.place(Kind::Design, &self.entry.id)?.join(id);
        #[cfg(not(target_family = "wasm"))]
        return url::Url::from_directory_path(folder).ok().map(Into::into);
        #[cfg(target_family = "wasm")]
        {
            let _ = folder;
            None
        }
    }

    fn close_artboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.take().is_none() {
            cx.propagate();
            return;
        }
        self.park();
        self.comments.close();
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
        self.pins = pins::Pins::default();
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
                self.live = Some((key, opened));
            }
            if artboard.revision.is_none() {
                self.live = None;
            }
            match self.live.as_ref().map(|(_, live)| live) {
                Some(Ok(live)) => {
                    let browser = live.browser.clone();
                    let fit = preview::fit(record.viewport, self.available);
                    let review = self.interactive_artboard(browser, artboard, fit, cx);
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
                    .children(shown_key.and_then(|artboard| self.comment_panel(artboard, cx)))
                    .child(
                        div()
                            .absolute()
                            .top(px(GUTTER))
                            .right(px(GUTTER))
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .children(shown_key.map(|artboard| self.expanded_tools(artboard, cx)))
                            .child(self.viewport_button.clone()),
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
