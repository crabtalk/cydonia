use crate::{
    model::{
        project::ProjectId,
        workspace::{Workspace, references},
    },
    view::{
        canvas::CanvasView,
        kit::{
            entry_search::Hit,
            link::mention::{self, Linkables},
        },
        search::{self, SETTLE},
    },
};
use artifact::{reference, search::Query};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyBinding,
        ScrollHandle, Task, Window, actions, div, point, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{
        icons,
        input::{FieldEvent, TextField},
        popover,
    },
};

const CONTEXT: &str = "CanvasEntryPicker";
const SHOWN: usize = 20;

actions!(canvas_entry_picker, [Next, Previous, Choose, Dismiss]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    [
        KeyBinding::new("down", Next, Some(CONTEXT)),
        KeyBinding::new("ctrl-n", Next, Some(CONTEXT)),
        KeyBinding::new("up", Previous, Some(CONTEXT)),
        KeyBinding::new("ctrl-p", Previous, Some(CONTEXT)),
        KeyBinding::new("enter", Choose, Some(CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
    ]
    .into()
}

pub(super) struct ReferencePicker {
    picker: Entity<EntryPicker>,
    at: (i64, i64),
}

struct EntryPicker {
    field: Entity<TextField>,
    workspace: Entity<Workspace>,
    project: ProjectId,
    hits: Vec<Hit>,
    selected: usize,
    scroll: ScrollHandle,
    task: Option<Task<()>>,
    searching: bool,
    error: Option<String>,
}

enum Event {
    Pick(String),
    Dismiss,
}

impl CanvasView {
    pub(crate) fn pick_reference(
        &mut self,
        replace: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.surface.read(cx).editor().options().read_only {
            return;
        }
        let Some(workspace) = references::current(cx) else {
            return;
        };
        let node = replace
            .as_ref()
            .and_then(|id| self.surface.read(cx).editor().canvas().node(id));
        let initial = node
            .and_then(|node| node.url.as_ref().or(node.file.as_ref()))
            .cloned()
            .unwrap_or_default();
        let at = node.map_or(self.at, |node| (node.x + node.width + 8, node.y));
        let project = self.canvas.project;
        let insertion = self.at;
        let picker = cx.new(|cx| EntryPicker::new(workspace, project, initial, cx));
        cx.subscribe_in(
            &picker,
            window,
            move |this, picker, event: &Event, window, cx| {
                this.at = insertion;
                if let Event::Pick(url) = event
                    && let Err(error) = this.reference(url, replace.clone(), cx)
                {
                    picker.update(cx, |picker, cx| {
                        picker.error = Some(error);
                        cx.notify();
                    });
                    return;
                }
                popover::close_popup(this, cx, |this| &mut this.reference_picker);
                window.focus(&this.focus_handle(cx), cx);
            },
        )
        .detach();
        window.focus(&picker.focus_handle(cx), cx);
        self.reference_picker.open(ReferencePicker { picker, at });
        cx.notify();
    }

    pub(super) fn reference_picker(&self, cx: &App) -> Option<AnyElement> {
        let opened = self.reference_picker.get()?;
        let surface = self.surface.read(cx);
        let origin = surface.bounds()?.origin;
        let editor = surface.editor();
        let pan = editor.pan();
        let zoom = editor.zoom();
        let position = point(
            origin.x + px(pan.x + opened.at.0 as f32 * zoom),
            origin.y + px(pan.y + opened.at.1 as f32 * zoom),
        );
        Some(popover::menu_at(
            "canvas-entry-picker",
            position,
            opened.picker.clone().into_any_element(),
            self.reference_picker.closing_since(),
        ))
    }
}

impl EntryPicker {
    fn new(
        workspace: Entity<Workspace>,
        project: ProjectId,
        initial: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = cx.new(|cx| {
            let mut field = TextField::new(cx)
                .with_frame(false)
                .with_key_context(CONTEXT)
                .with_placeholder("Search entries or enter #12#heading…");
            field.set_content(initial, cx);
            field
        });
        cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                this.refresh(cx);
            }
        })
        .detach();
        let mut this = Self {
            field,
            workspace,
            project,
            hits: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            task: None,
            searching: false,
            error: None,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.error = None;
        self.searching = false;
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        let text = self.field.read(cx).content().to_string();
        let workspace = self.workspace.read(cx);
        if text.trim().is_empty() {
            let mut entries = cx
                .try_global::<Linkables>()
                .map_or(Vec::new(), |held| held.0.iter().collect::<Vec<_>>());
            entries.sort_by_key(|entry| {
                (
                    entry.row.project() != Some(self.project),
                    std::cmp::Reverse(entry.touched),
                )
            });
            self.hits = Hit::recent(
                &entries
                    .into_iter()
                    .take(SHOWN)
                    .map(|entry| entry.row.clone())
                    .collect::<Vec<_>>(),
            );
        } else {
            self.hits = Hit::named(&text, workspace, Some(self.project), cx);
            if reference::parse(text.trim()).is_none()
                && let Some(query) = Query::literal(mention::Query::parse(&text).rest)
            {
                self.searching = true;
                let roots = workspace
                    .projects
                    .iter()
                    .map(|open| open.path.clone())
                    .collect::<Vec<_>>();
                self.task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SETTLE).await;
                    let found = cx
                        .background_executor()
                        .spawn(async move { search::search_all(&roots, &query) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        let selected = this
                            .hits
                            .get(this.selected)
                            .and_then(|hit| hit.link(this.workspace.read(cx), cx));
                        this.hits = Hit::matching(
                            &text,
                            found,
                            this.workspace.read(cx),
                            Some(this.project),
                            cx,
                        );
                        this.retain_linkable(cx);
                        this.searching = false;
                        this.selected = selected
                            .and_then(|url| {
                                this.hits.iter().position(|hit| {
                                    hit.link(this.workspace.read(cx), cx).as_ref() == Some(&url)
                                })
                            })
                            .unwrap_or(0);
                        cx.notify();
                    });
                }));
            }
        }
        self.retain_linkable(cx);
        cx.notify();
    }

    fn retain_linkable(&mut self, cx: &App) {
        let workspace = self.workspace.read(cx);
        let kind = mention::Query::parse(self.field.read(cx).content()).kind;
        self.hits.retain(|hit| {
            kind.is_none_or(|kind| hit.row.kind() == Some(kind))
                && hit.link(workspace, cx).is_some()
        });
        self.hits.truncate(SHOWN);
    }

    fn step(&mut self, by: isize, cx: &mut Context<Self>) {
        if self.hits.is_empty() {
            return;
        }
        self.selected = (self.selected as isize + by).rem_euclid(self.hits.len() as isize) as usize;
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn choose(&mut self, ix: usize, cx: &mut Context<Self>) {
        let url = self
            .hits
            .get(ix)
            .and_then(|hit| hit.link(self.workspace.read(cx), cx));
        if let Some(url) = url {
            cx.emit(Event::Pick(url));
        } else if reference::parse(self.field.read(cx).content().trim()).is_some() {
            cx.emit(Event::Pick(self.field.read(cx).content().trim().to_owned()));
        }
    }
}

impl EventEmitter<Event> for EntryPicker {}
impl Focusable for EntryPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.field.focus_handle(cx)
    }
}
impl Render for EntryPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let rows = self
            .hits
            .iter()
            .enumerate()
            .map(|(ix, hit)| {
                hit.render(ix, ix == self.selected, self.workspace.read(cx), cx)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.choose(ix, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        popover::popover_card_flush(&theme)
            .w(px(400.).min((window.viewport_size().width - px(24.)).max(px(0.))))
            .max_h(px(320.).min((window.viewport_size().height - px(24.)).max(px(0.))))
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_down_out(cx.listener(|_, _, _, cx| {
                cx.emit(Event::Dismiss);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Previous, _, cx| this.step(-1, cx)))
            .on_action(cx.listener(|this, _: &Choose, _, cx| this.choose(this.selected, cx)))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(Event::Dismiss)))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        icons::icon(icons::text::Search)
                            .size(px(14.))
                            .text_color(theme.text_faint),
                    )
                    .child(div().flex_1().min_w_0().child(self.field.clone())),
            )
            .child(
                div()
                    .id("entry-picker-results")
                    .min_h_0()
                    .map(|el| bezel::ui::scroll::scrolls(el, bezel::ui::scroll::Axes::Vertical))
                    .track_scroll(&self.scroll)
                    .p(px(6.))
                    .flex()
                    .flex_col()
                    .children(rows)
                    .when(self.hits.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(8.))
                                .py(px(10.))
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_faint)
                                .child(if self.searching {
                                    "Searching…"
                                } else {
                                    "No entries found"
                                }),
                        )
                    }),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .px(px(12.))
                    .pb(px(8.))
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.danger_muted)
                    .child(error)
            }))
    }
}
