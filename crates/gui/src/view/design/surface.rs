use crate::view::{
    design::{
        DesignView,
        preview::{self, Revision},
    },
    kit::{canvas::Navigation, web},
};
use artifact::{canvas::web::Source, design::Design, kind::Kind};
use bezel::{
    gpui::{
        AnyElement, AppContext as _, Context, Entity, ObjectFit, SharedString, StyledImage as _,
        Window, div, img, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        icons,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use canvas::{
    CanvasEditor, CanvasView as Surface, Document,
    kind::{self, Card, Kinds},
    layout,
    model::{Canvas, Node},
};

const ARTBOARD: &str = "artboard";
const FRAME_GUTTER: i64 = 120;
const ROW_WIDTH: i64 = 5_000;

impl DesignView {
    pub(super) fn make_surface(cx: &mut Context<Self>) -> (Entity<Document>, Entity<Surface>) {
        let document = cx.new(|_| Document::new(Canvas::default()));
        let owner = cx.weak_entity();
        let kinds = Kinds::new().with(
            ARTBOARD,
            kind::Kind::new(move |node, look, _, cx| {
                owner
                    .upgrade()
                    .map(|view| view.read(cx).frame(node, &look.card, &view, cx))
                    .unwrap_or_else(|| div().into_any_element())
            }),
        );
        let surface = cx.new(|cx| {
            Surface::for_document(document.clone(), layout::FREE, cx)
                .with_kinds(kinds)
                .with_options(canvas::Options {
                    read_only: true,
                    ..Default::default()
                })
        });
        (document, surface)
    }

    pub(super) fn canvas(&mut self, record: &Design, cx: &mut Context<Self>) -> AnyElement {
        let width = i64::from(record.viewport.width);
        let (mut x, mut y, mut row_height) = (0, 0, 0);
        let nodes = record
            .artboards
            .iter()
            .map(|artboard| {
                let height = i64::from(
                    Revision::new(artboard, record.viewport)
                        .and_then(|key| self.thumbnails.get(&key)?.as_ref().ok())
                        .map_or(record.viewport.height, |preview| preview.height),
                );
                if x > 0 && x + width > ROW_WIDTH {
                    x = 0;
                    y += row_height + FRAME_GUTTER;
                    row_height = 0;
                }
                let node = Node {
                    id: artboard.id.clone(),
                    kind: ARTBOARD.into(),
                    label: Some(artboard.title.clone()),
                    x,
                    y,
                    width,
                    height,
                    ..Default::default()
                };
                x += width + FRAME_GUTTER;
                row_height = row_height.max(height);
                node
            })
            .collect();
        self.document.update(cx, |document, cx| {
            document.reload(
                Canvas {
                    nodes,
                    ..Default::default()
                },
                cx,
            )
        });
        if !self.fitted
            && self.available.width > px(0.)
            && !record.artboards.is_empty()
            && self.loading.is_none()
        {
            self.fitted = true;
            let surface = self.surface.clone();
            cx.defer(move |cx| {
                surface.update(cx, |surface, cx| {
                    surface.update_editor(cx, CanvasEditor::fit)
                });
            });
        }
        div()
            .size_full()
            .relative()
            .child(self.surface.clone())
            .when(record.artboards.is_empty(), |view| {
                view.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_style(TextStyle::Callout)
                        .text_color(Theme::of(cx).text_muted)
                        .child("Ask an agent to add an artboard."),
                )
            })
            .child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .right(px(12.))
                    .child(Navigation(self.surface.clone()).render(cx)),
            )
            .into_any_element()
    }

    fn frame(
        &self,
        node: &Node,
        card: &Card,
        owner: &Entity<Self>,
        cx: &bezel::gpui::App,
    ) -> AnyElement {
        let record = self.source.read(cx);
        let Some(artboard) = record
            .artboards
            .iter()
            .find(|artboard| artboard.id == node.id)
        else {
            return div().into_any_element();
        };
        let theme = Theme::of(cx);
        let key = Revision::new(artboard, record.viewport);
        let image = key.as_ref().and_then(|key| self.thumbnails.get(key));
        let live = self
            .hover
            .as_ref()
            .filter(|(revision, _, _)| Some(revision) == key.as_ref());
        let id = node.id.clone();
        let hover_id = id.clone();
        let weak = owner.downgrade();
        let expand = weak.clone();
        let comment = weak.clone();
        let artboard_id = id.clone();
        let comments_id = id.clone();
        let external = owner.downgrade();
        let path = artboard.path();
        let body = div()
            .id(SharedString::from(id.clone()))
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme.surface_card)
            .on_hover(move |over, window, cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.hover_artboard(&hover_id, *over, window, cx)
                });
            })
            .child(match image {
                Some(Ok(preview)) => img(preview.image.clone())
                    .size_full()
                    .object_fit(ObjectFit::Fill)
                    .into_any_element(),
                state => div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(
                        match (state, key.as_ref()) {
                            (Some(Err(error)), _) => error.as_str(),
                            (_, Some(_)) => "Rendering preview…",
                            _ => "Waiting for index.html",
                        }
                        .to_owned(),
                    )
                    .into_any_element(),
            })
            .children(live.map(|(_, live, _)| {
                div()
                    .absolute()
                    .inset_0()
                    .when(!live.is_ready(cx), |host| host.opacity(0.01))
                    .child(live.browser.clone())
            }));
        let extra = px((FRAME_GUTTER as f32 * card.zoom - 6.).max(0.));
        let header_width = card.size.width + extra.min((px(210.) - card.size.width).max(px(0.)));
        div()
            .size_full()
            .relative()
            .child(body)
            .child(
                web::frame_header(&artboard.title, cx)
                    .w(header_width)
                    .child(
                        theme
                            .icon_button(icons::glyph::MessageCirclePlus, ButtonStyle::Ghost, None)
                            .id("artboard-comment")
                            .w(px(22.))
                            .flex_none()
                            .tooltip(|window, cx| {
                                Tooltip::text("Comment on an element", window, cx)
                            })
                            .on_click(move |_, window, cx| {
                                let _ = comment.update(cx, |view, cx| {
                                    view.expand(artboard_id.clone(), window, cx);
                                    view.toggle_picking(window, cx);
                                });
                            }),
                    )
                    .child(
                        Self::comments_button(artboard.comments.len(), cx).on_click({
                            let owner = owner.downgrade();
                            move |_, window, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    view.expand(comments_id.clone(), window, cx);
                                    view.open_comments(cx);
                                });
                            }
                        }),
                    )
                    .child(
                        theme
                            .icon_button(icons::arrows::Maximize2, ButtonStyle::Ghost, None)
                            .id("artboard-expand")
                            .w(px(22.))
                            .flex_none()
                            .tooltip(|window, cx| Tooltip::text("Expand artboard", window, cx))
                            .on_click(move |_, window, cx| {
                                let _ = expand
                                    .update(cx, |view, cx| view.expand(id.clone(), window, cx));
                            }),
                    )
                    .child(
                        theme
                            .icon_button(icons::arrows::ExternalLink, ButtonStyle::Ghost, None)
                            .id("artboard-browser")
                            .w(px(22.))
                            .flex_none()
                            .tooltip(|window, cx| Tooltip::text("Open in browser", window, cx))
                            .on_click(move |_, _, cx| {
                                if let Some(url) = external.upgrade().and_then(|view| {
                                    let view = view.read(cx);
                                    let folder = view.store.place(Kind::Design, &view.entry.id)?;
                                    Some(
                                        Source::try_from(path.clone())
                                            .ok()?
                                            .resolve(&folder)
                                            .ok()?
                                            .url,
                                    )
                                }) {
                                    cx.open_url(&url);
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    fn hover_artboard(
        &mut self,
        id: &str,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !over {
            if self
                .hover
                .as_ref()
                .is_some_and(|(key, _, _)| key.artboard == id)
            {
                self.hover = None;
                cx.notify();
            }
            return;
        }
        if self.open.is_some() {
            return;
        }
        let record = self.source.read(cx);
        let Some(artboard) = record
            .artboards
            .iter()
            .find(|artboard| artboard.id == id)
            .cloned()
        else {
            return;
        };
        let viewport = record.viewport;
        let Some(key) = Revision::new(&artboard, viewport) else {
            return;
        };
        let Some(Ok(preview)) = self.thumbnails.get(&key) else {
            return;
        };
        let full = artifact::web::Viewport {
            height: preview.height,
            ..viewport
        };
        if let Some(folder) = self.store.place(Kind::Design, &self.entry.id)
            && let Ok(page) = preview::open(&folder, &artboard, full, false, window, cx)
        {
            let events = cx.observe(&page.browser, |_, _, cx| cx.notify());
            self.hover = Some((key, page, events));
            cx.notify();
        }
    }
}
