use crate::{
    model::{store::Store, workspace::Entry},
    view::design::preview::{LivePage, Revision},
};
use artifact::{
    design::{Design, Page, Status},
    kind::Kind,
    project::Project as _,
    web::Viewport,
};
use bezel::{
    gpui::{
        self, AnyElement, App, ClickEvent, Context, Entity, FocusHandle, Focusable, KeyBinding,
        ObjectFit, Render, RenderImage, ScrollHandle, Size, StyledImage as _, Task, Window,
        actions, div, img, prelude::*, px, size,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        scroll,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use browser::WebView;
use std::{collections::HashMap, sync::Arc};

mod comments;
mod pins;
pub(crate) mod preview;

const CONTEXT: &str = "DesignReview";
const TILE_WIDTH: f32 = 236.;
const TILE_HEIGHT: f32 = 158.;
const GUTTER: f32 = 16.;
const MAX_COLUMNS: usize = 5;

actions!(
    design_review,
    [Left, Right, Up, Down, Open, Back, Approve, RequestChanges]
);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    let context = Some(CONTEXT);
    vec![
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("enter", Open, context),
        KeyBinding::new("escape", Back, context),
        KeyBinding::new("a", Approve, context),
        KeyBinding::new("c", RequestChanges, context),
    ]
}

pub(crate) struct DesignView {
    pub(crate) entry: Entry,
    source: Entity<Design>,
    store: Store,
    focus: FocusHandle,
    selected: Option<String>,
    open: Option<String>,
    pending_only: bool,
    grid_scroll: ScrollHandle,
    available: Size<gpui::Pixels>,
    thumbnails: HashMap<Revision, Result<Arc<RenderImage>, String>>,
    loading: Option<(Revision, Entity<WebView>, Task<()>)>,
    live: Option<(Revision, Result<LivePage, String>)>,
    error: Option<String>,
    pins: pins::Pins,
    comments: comments::Review,
}

impl DesignView {
    pub(crate) fn new(
        entry: Entry,
        source: Entity<Design>,
        store: Store,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        Self {
            entry,
            source,
            store,
            focus: cx.focus_handle(),
            selected: None,
            open: None,
            pending_only: false,
            grid_scroll: ScrollHandle::new(),
            available: size(px(0.), px(0.)),
            thumbnails: HashMap::new(),
            loading: None,
            live: None,
            error: None,
            pins: pins::Pins::default(),
            comments: comments::Review::new(cx),
        }
    }

    fn visible<'a>(&self, record: &'a Design) -> Vec<&'a Page> {
        record
            .pages
            .iter()
            .filter(|page| !self.pending_only || page.status() != Status::Approved)
            .collect()
    }

    fn columns(&self) -> usize {
        ((self.available.width.as_f32() - GUTTER) / (TILE_WIDTH + GUTTER))
            .floor()
            .clamp(1., MAX_COLUMNS as f32) as usize
    }

    fn move_selection(&mut self, offset: isize, cx: &mut Context<Self>) {
        if self.open.is_some() {
            cx.propagate();
            return;
        }
        let pages = self.visible(self.source.read(cx));
        let Some(at) = pages
            .iter()
            .position(|page| Some(&page.id) == self.selected.as_ref())
        else {
            return;
        };
        let next = at.saturating_add_signed(offset).min(pages.len() - 1);
        self.selected = Some(pages[next].id.clone());
        self.grid_scroll.scroll_to_item(next / self.columns());
        cx.notify();
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.open.is_none() {
            self.open = self.selected.clone();
            self.loading = None;
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn open_browser(&self) -> Option<Entity<WebView>> {
        self.live
            .as_ref()
            .and_then(|(_, live)| live.as_ref().ok())
            .map(|live| live.browser.clone())
    }

    pub(crate) fn live_page(&self, page: &Page, viewport: Viewport) -> Option<LivePage> {
        let (held, live) = self.live.as_ref()?;
        (Some(held) == Revision::new(page, viewport).as_ref())
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

    fn close_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.take().is_none() {
            cx.propagate();
            return;
        }
        self.park();
        self.comments.close();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn write(&mut self, edit: impl FnOnce(&mut Design), cx: &mut Context<Self>) {
        let Some(mut record) = self.store.design(&self.entry.id) else {
            self.error = Some("This design is no longer available.".into());
            cx.notify();
            return;
        };
        edit(&mut record);
        self.error = match self.store.save_design(&mut record) {
            Ok(()) => {
                self.source.update(cx, |design, cx| {
                    *design = record;
                    cx.notify();
                });
                None
            }
            Err(error) => Some(error.to_string()),
        };
        cx.notify();
    }

    fn review(&mut self, status: Status, cx: &mut Context<Self>) {
        let record = self.source.read(cx);
        let selected = self.open.as_ref().or(self.selected.as_ref());
        let Some(page) = record.pages.iter().find(|page| Some(&page.id) == selected) else {
            return;
        };
        if page.revision.is_none() {
            return;
        }
        let (id, revision) = (page.id.clone(), page.revision.clone());
        self.write(
            move |record| {
                if let Some(page) = record.pages.iter_mut().find(|page| page.id == id)
                    && page.revision == revision
                {
                    page.review_at(status);
                }
            },
            cx,
        );
    }

    fn advance_preview(
        &mut self,
        record: &Design,
        folder: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pages = self.visible(record);
        let revisions: Vec<_> = pages
            .iter()
            .filter_map(|page| Revision::new(page, record.viewport))
            .collect();
        if self
            .loading
            .as_ref()
            .is_some_and(|(key, _, _)| !revisions.contains(key))
        {
            self.loading = None;
        }
        if self.loading.is_some() {
            return;
        }
        for (page, key) in pages
            .iter()
            .filter_map(|page| Revision::new(page, record.viewport).map(|key| (*page, key)))
        {
            if self.thumbnails.contains_key(&key) {
                continue;
            }
            match preview::open(folder, page, record.viewport, false, window, cx) {
                Ok(live) => {
                    let browser = live.browser;
                    let capturing = browser.clone();
                    let captured = key.clone();
                    let task = cx.spawn(async move |this, cx| {
                        let image = preview::snapshot(&capturing, cx).await;
                        let _ = this.update(cx, |this, cx| {
                            if this
                                .loading
                                .as_ref()
                                .is_some_and(|(key, _, _)| key == &captured)
                            {
                                this.thumbnails.insert(captured, image);
                                this.loading = None;
                                cx.notify();
                            }
                        });
                    });
                    self.loading = Some((key, browser, task));
                    return;
                }
                Err(error) => {
                    self.thumbnails.insert(key, Err(error));
                }
            }
        }
    }

    fn status(&self, page: &Page, cx: &App) -> impl IntoElement {
        let theme = Theme::of(cx);
        let (label, color) = match page.status() {
            Status::New => ("New", theme.accent),
            Status::Approved => ("Approved", theme.text_muted),
            Status::Changes => ("Changes requested", theme.danger),
        };
        div()
            .flex_none()
            .text_style(TextStyle::Caption)
            .text_color(color)
            .child(label)
    }

    pub(crate) fn park(&mut self) {
        self.live = None;
        self.loading = None;
        self.pins = pins::Pins::default();
    }

    fn tile(&self, page: &Page, viewport: Viewport, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx);
        let selected = self.selected.as_ref() == Some(&page.id);
        let key = Revision::new(page, viewport);
        let preview = match key.as_ref().and_then(|key| self.thumbnails.get(key)) {
            Some(Ok(image)) => img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            result => div()
                .text_style(TextStyle::Callout)
                .text_color(theme.text_muted)
                .child(match (result, key) {
                    (Some(Err(error)), _) => error.clone(),
                    (_, Some(_)) => "Rendering preview…".into(),
                    _ => "Waiting for index.html".into(),
                })
                .into_any_element(),
        };
        let status = self.status(page, cx);
        let id = page.id.clone();
        div()
            .id(gpui::SharedString::from(id.clone()))
            .min_w_0()
            .overflow_hidden()
            .rounded(px(6.))
            .border_1()
            .border_color(if selected { theme.accent } else { theme.border })
            .bg(theme.surface_card)
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.selected = Some(id.clone());
                if event.click_count() > 1 {
                    this.open_selected(cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .h(px(TILE_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(preview),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(theme.border)
                    .p(px(10.))
                    .gap(px(8.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w_0()
                            .text_style(TextStyle::Callout)
                            .truncate()
                            .child(page.title.clone()),
                    )
                    .child(status),
            )
            .into_any_element()
    }
}

impl Focusable for DesignView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DesignView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let record = self.source.read(cx).clone();
        let theme = Theme::of(cx).clone();
        let folder = self.store.place(Kind::Design, &record.id);
        let shown_key = self
            .open
            .as_ref()
            .and_then(|id| record.pages.iter().find(|page| &page.id == id));
        let revisions: Vec<_> = record
            .pages
            .iter()
            .filter_map(|page| Revision::new(page, record.viewport))
            .collect();
        self.thumbnails.retain(|key, _| revisions.contains(key));
        if self.open.is_some() && shown_key.is_none() {
            self.open = None;
            self.live = None;
        }
        let pages = self.visible(&record);
        if !pages
            .iter()
            .any(|page| Some(&page.id) == self.selected.as_ref())
        {
            self.selected = pages.first().map(|page| page.id.clone());
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
        let body = if let Some(page) = shown_key {
            let key = Revision::new(page, record.viewport);
            if let Some(key) = key
                && self.live.as_ref().is_none_or(|(held, _)| held != &key)
            {
                let opened = folder
                    .as_deref()
                    .ok_or_else(|| "Design directory unavailable".into())
                    .and_then(|folder| {
                        preview::open(folder, page, record.viewport, true, window, cx)
                    });
                self.live = Some((key, opened));
            }
            if page.revision.is_none() {
                self.live = None;
            }
            match self.live.as_ref().map(|(_, live)| live) {
                Some(Ok(live)) => {
                    let browser = live.browser.clone();
                    let fit = preview::fit(record.viewport, self.available);
                    let review = self.review_surface(browser, page, fit, cx);
                    div()
                        .size_full()
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
            if pages.is_empty() {
                div()
                    .p(px(GUTTER))
                    .text_style(TextStyle::Callout)
                    .child(if record.pages.is_empty() {
                        "Ask an agent to add a page."
                    } else {
                        "Every page is approved."
                    })
                    .into_any_element()
            } else {
                scroll::scrolls(
                    div()
                        .id("design-grid")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.grid_scroll),
                    scroll::Axes::Vertical,
                )
                .flex()
                .flex_col()
                .gap(px(GUTTER))
                .p(px(GUTTER))
                .children(pages.chunks(self.columns()).map(|row| {
                    div()
                        .grid()
                        .w_full()
                        .grid_cols(self.columns() as u16)
                        .gap(px(GUTTER))
                        .children(row.iter().map(|page| self.tile(page, record.viewport, cx)))
                }))
                .into_any_element()
            }
        };
        let selected = shown_key.or_else(|| {
            pages
                .iter()
                .copied()
                .find(|page| Some(&page.id) == self.selected.as_ref())
        });
        let can_review = selected.is_some_and(|page| page.revision.is_some());
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &Left, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| {
                this.move_selection(-(this.columns() as isize), cx)
            }))
            .on_action(
                cx.listener(|this, _: &Down, _, cx| {
                    this.move_selection(this.columns() as isize, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Open, _, cx| this.open_selected(cx)))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.close_page(window, cx)))
            .on_action(cx.listener(|this, _: &Approve, _, cx| this.review(Status::Approved, cx)))
            .on_action(
                cx.listener(|this, _: &RequestChanges, _, cx| this.review(Status::Changes, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(4.))
                    .px(px(GUTTER))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        theme
                            .button(
                                if self.open.is_some() { "Back" } else { "Open" },
                                ButtonStyle::Ghost,
                                None,
                            )
                            .id("design-open")
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.open.is_some() {
                                    this.close_page(window, cx);
                                } else {
                                    this.open_selected(cx);
                                }
                            })),
                    )
                    .children(
                        ["Desktop", "Tablet", "Phone"]
                            .into_iter()
                            .zip(Viewport::PRESETS)
                            .map(|(label, viewport)| {
                                theme
                                    .button(
                                        label,
                                        if record.viewport == viewport {
                                            ButtonStyle::Secondary
                                        } else {
                                            ButtonStyle::Ghost
                                        },
                                        None,
                                    )
                                    .id(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.write(|record| record.viewport = viewport, cx)
                                    }))
                            }),
                    )
                    .children(shown_key.map(|page| {
                        div()
                            .min_w_0()
                            .flex_1()
                            .px(px(8.))
                            .gap(px(8.))
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .text_style(TextStyle::Callout)
                                    .truncate()
                                    .child(page.title.clone()),
                            )
                            .child(self.status(page, cx))
                    }))
                    .when(shown_key.is_none(), |bar| bar.child(div().flex_1()))
                    .when(self.open.is_none(), |bar| {
                        bar.child(
                            theme
                                .button(
                                    if self.pending_only {
                                        "Pending review"
                                    } else {
                                        "All pages"
                                    },
                                    ButtonStyle::Ghost,
                                    None,
                                )
                                .id("design-filter")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.pending_only = !this.pending_only;
                                    cx.notify();
                                })),
                        )
                    })
                    .when(shown_key.is_some(), |bar| {
                        bar.when(self.open_browser().is_some(), |bar| {
                            bar.child(
                                theme
                                    .button(
                                        if self.pins.picking {
                                            "Cancel"
                                        } else {
                                            "Comment"
                                        },
                                        ButtonStyle::Ghost,
                                        None,
                                    )
                                    .id("design-comment")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_picking(window, cx)
                                    })),
                            )
                        })
                        .when(
                            shown_key.is_some_and(|page| !page.comments.is_empty()),
                            |bar| {
                                bar.child(
                                    theme
                                        .button("Comments", ButtonStyle::Ghost, None)
                                        .id("design-comments")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.open_comments(cx)),
                                        ),
                                )
                            },
                        )
                    })
                    .when(can_review, |bar| {
                        bar.child(
                            theme
                                .button("Approve", ButtonStyle::Ghost, None)
                                .id("design-approve")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.review(Status::Approved, cx)),
                                ),
                        )
                        .child(
                            theme
                                .button("Request changes", ButtonStyle::Ghost, None)
                                .id("design-changes")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.review(Status::Changes, cx)),
                                ),
                        )
                    }),
            )
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
                    .children(shown_key.and_then(|page| self.comment_panel(page, cx)))
                    .children(
                        self.loading
                            .as_ref()
                            .map(|(_, browser, _)| preview::host(browser.clone())),
                    ),
            )
    }
}
