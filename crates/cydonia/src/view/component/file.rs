//! Small text-file buffers with explicit saves and external-change detection.

use crate::model::typography;
use bezel::{
    gpui::{
        self, Context, Entity, Focusable, Render, Subscription, Task, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::input::{FieldEvent, Shape, TextField},
};
use std::{
    cell::Cell,
    io::{Read, Write},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

const LIMIT: u64 = 256 * 1024;
/// How long a keystroke waits before the file is parsed again. Every edit
/// re-parses the whole file — the field holds text, not a syntax tree — so a
/// run of typing coalesces into one parse instead of one per character.
const RECOLOUR: Duration = Duration::from_millis(40);
gpui::actions!(
    file_editor,
    [Save, IncreaseTextSize, DecreaseTextSize, ResetTextSize]
);

pub(crate) fn read_text(path: &Path) -> anyhow::Result<String> {
    anyhow::ensure!(std::fs::metadata(path)?.is_file(), "Choose a regular file");
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= LIMIT,
        "Files larger than 256 KiB must be opened externally"
    );
    anyhow::ensure!(
        !bytes.contains(&0),
        "Binary files must be opened externally"
    );
    Ok(String::from_utf8(bytes)?)
}

pub(crate) fn write_text(
    path: &Path,
    saved: &str,
    source: &str,
    overwrite: bool,
) -> anyhow::Result<()> {
    if !overwrite {
        anyhow::ensure!(
            read_text(path)? == saved,
            "File changed on disk. Reload or overwrite to continue."
        );
    }
    anyhow::ensure!(
        source.len() as u64 <= LIMIT,
        "Files larger than 256 KiB must be edited externally"
    );
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".cydonia-save-{}-{sequence}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> anyhow::Result<()> {
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(source.as_bytes())?;
        file.sync_all()?;
        if !overwrite {
            anyhow::ensure!(
                read_text(path)? == saved,
                "File changed on disk. Reload or overwrite to continue."
            );
        }
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn normalized(source: &str) -> String {
    source.replace("\r\n", "\n").replace('\r', "\n")
}

fn line_endings(source: &str, saved: &str) -> String {
    if saved.contains("\r\n") && !saved.replace("\r\n", "").contains('\n') {
        source.replace('\n', "\r\n")
    } else if saved.contains('\r') && !saved.contains('\n') {
        source.replace('\n', "\r")
    } else {
        source.to_string()
    }
}

pub struct FileView {
    pub root: PathBuf,
    focus: gpui::FocusHandle,
    pub path: PathBuf,
    pub field: Entity<TextField>,
    saved: String,
    ready: bool,
    loading: bool,
    pub error: Option<String>,
    changed: bool,
    preview: bool,
    preview_selection: Option<markdown::Selection>,
    preview_layouts: markdown::BlockLayouts,
    preview_dragging: bool,
    scroll: gpui::ScrollHandle,
    reveal: Rc<Cell<bool>>,
    target_line: Rc<Cell<Option<usize>>>,
    /// What this file's name says it is, resolved once: the path a view is
    /// opened on does not change under it.
    pub(super) language: Option<crate::model::language::Language>,
    _watch: Subscription,
    _poll: Task<()>,
    /// The parse in flight. Replaced by the next edit, which drops it — that
    /// is the debounce.
    _recolour: Task<()>,
}

impl FileView {
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_frame(false)
                .with_shape(Shape::Grow {
                    min: 1,
                    max: usize::MAX,
                })
                .with_undo_limit(64)
                .with_key_context("FileEditor")
        });
        let watch = cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed | FieldEvent::Moved) {
                this.reveal.set(true);
            }
            // Loading a file emits this too — `set_content` is an edit as far
            // as the field is concerned — so first paint is coloured by the
            // same path that keeps typing coloured.
            if matches!(event, FieldEvent::Changed) {
                this.preview_selection = None;
                this.preview_dragging = false;
                this.recolour(cx);
            }
            cx.notify();
        });
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let Ok((path, saved)) =
                    this.update(cx, |this, _| (this.path.clone(), this.saved.clone()))
                else {
                    return;
                };
                let result = cx
                    .background_executor()
                    .spawn(async move { read_text(&path) })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.saved == saved {
                            this.receive(result, cx);
                        }
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(Duration::from_secs(3)).await;
            }
        });
        Self {
            root: path.parent().unwrap_or(Path::new("/")).to_path_buf(),
            focus: cx.focus_handle(),
            language: crate::model::language::of(&path),
            path,
            field,
            saved: String::new(),
            ready: false,
            loading: true,
            error: None,
            changed: false,
            preview: true,
            preview_selection: None,
            preview_layouts: markdown::BlockLayouts::default(),
            preview_dragging: false,
            scroll: gpui::ScrollHandle::new(),
            reveal: Rc::new(Cell::new(false)),
            target_line: Rc::new(Cell::new(None)),
            _watch: watch,
            _poll: poll,
            _recolour: Task::ready(()),
        }
    }

    /// Parse the file again and hand the spans to the field.
    ///
    /// Off the main thread: the largest file this view will open is tens of
    /// milliseconds of tree-sitter, and this runs from a keystroke. The field keeps painting the spans it
    /// already has until these arrive, so the text never flashes plain
    /// mid-edit.
    fn recolour(&mut self, cx: &mut Context<Self>) {
        use crate::model::language::Language;
        // A language nothing here can paint is not worth a parse, and a file
        // whose name names nothing at all is not worth asking about.
        if !matches!(self.language, Some(Language::Ready(_) | Language::Markdown)) {
            return;
        }
        let path = self.path.clone();
        let source = self.field.read(cx).content().clone();
        self._recolour = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOLOUR).await;
            let spans =
                cx.background_executor()
                    .spawn(async move {
                        crate::model::language::spans(&path, &source).unwrap_or_default()
                    })
                    .await;
            let _ = this.update(cx, |this, cx| {
                this.field
                    .update(cx, |field, cx| field.set_spans(spans, cx));
            });
        });
    }

    pub(super) fn receive(&mut self, result: anyhow::Result<String>, cx: &mut Context<Self>) {
        self.loading = false;
        match result {
            Ok(source) => {
                if !self.ready || !self.dirty(cx) {
                    if !self.ready || self.saved != source {
                        self.field
                            .update(cx, |field, cx| field.set_content(source.clone(), cx));
                        self.saved = source;
                    }
                    self.changed = false;
                    self.error = None;
                } else {
                    self.changed = source != self.saved;
                }
                self.ready = true;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(super) fn draft_snapshot(&self, cx: &gpui::App) -> Option<(String, String)> {
        self.dirty(cx).then(|| {
            (
                self.saved.clone(),
                self.field.read(cx).content().to_string(),
            )
        })
    }

    pub(super) fn restore_draft(
        &mut self,
        (saved, draft): (String, String),
        cx: &mut Context<Self>,
    ) {
        self.saved = saved;
        self.ready = true;
        self.loading = false;
        self.field
            .update(cx, |field, cx| field.set_content(draft, cx));
        cx.notify();
    }

    pub fn dirty(&self, cx: &gpui::App) -> bool {
        self.ready && self.field.read(cx).content().as_ref() != normalized(&self.saved)
    }

    pub fn save(&mut self, overwrite: bool, cx: &mut Context<Self>) -> bool {
        if !self.ready {
            return false;
        }
        let source = if self.dirty(cx) {
            line_endings(self.field.read(cx).content(), &self.saved)
        } else {
            self.saved.clone()
        };
        match write_text(&self.path, &self.saved, &source, overwrite) {
            Ok(()) => {
                self.saved = source;
                self.changed = false;
                self.error = None;
                cx.notify();
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                false
            }
        }
    }

    pub fn status_bar(
        &mut self,
        files_open: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = Theme::of(cx).clone();
        let relative = self.path.strip_prefix(&self.root).unwrap_or(&self.path);
        let root_name = self
            .root
            .file_name()
            .unwrap_or(self.root.as_os_str())
            .to_string_lossy();
        let breadcrumb = format!(
            "{}  ›  {}",
            root_name,
            relative.display().to_string().replace('/', "  ›  ")
        );
        let markdown = self
            .path
            .extension()
            .is_some_and(|ext| ext == "md" || ext == "markdown");
        super::status::bar(&theme)
            .child(super::status::path(&self.path, breadcrumb))
            .when(markdown && self.ready, |row| {
                row.child(
                    div()
                        .id("file-mode")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.preview = !this.preview;
                            this.preview_dragging = false;
                            if this.preview {
                                window.focus(&this.focus, cx);
                            } else {
                                window.focus(&this.field.focus_handle(cx), cx);
                            }
                            cx.notify();
                        }))
                        .child(if self.preview {
                            "View source"
                        } else {
                            "Preview"
                        }),
                )
            })
            .child(super::status::files_toggle(files_open, &theme))
            .when(self.dirty(cx), |row| {
                row.child(
                    div()
                        .id("file-save")
                        .cursor_pointer()
                        .child("Save •")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save(false, cx);
                        })),
                )
            })
            .into_any_element()
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match read_text(&self.path) {
            Ok(source) => {
                self.field
                    .update(cx, |field, cx| field.set_content(source.clone(), cx));
                self.saved = source;
                self.ready = true;
                self.changed = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

impl FileView {
    pub(super) fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.preview = false;
        self.target_line.set(Some(line.max(1)));
        cx.notify();
    }

    fn source_view(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let starts = line_starts(self.field.read(cx).content());
        let size = typography::file_size(cx);
        let gutter = px(starts.len().to_string().len() as f32 * size * 0.65 + 20.);
        let field = self.field.clone();
        let scroll = self.scroll.clone();
        let reveal = self.reveal.clone();
        let target_line = self.target_line.clone();
        let ready = self.ready;
        let font = gpui::font(theme.font_mono.clone());
        let color = theme.text_faint;
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .id("file-source")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .pl(gutter + px(8.))
                    .pr(px(8.))
                    .py(px(8.))
                    .font_family(theme.font_mono.clone())
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            window.focus(&this.field.focus_handle(cx), cx);
                        }),
                    )
                    .child(self.field.clone()),
            )
            .child(
                gpui::canvas(
                    |bounds, _, _| bounds,
                    move |_, bounds, window, cx| {
                        let field = field.read(cx);
                        if ready
                            && let Some(line) = target_line.get()
                            && let Some(at) = starts
                                .get(line.saturating_sub(1).min(starts.len().saturating_sub(1)))
                            && let Some(row) = field.offset_bounds(*at)
                        {
                            let offset = scroll.offset();
                            let y = (offset.y + scroll.bounds().top() - row.top())
                                .clamp(-scroll.max_offset().y, px(0.));
                            scroll.set_offset(gpui::point(offset.x, y));
                            target_line.set(None);
                            reveal.set(false);
                            window.refresh();
                        }
                        if reveal.replace(false)
                            && field.focus_handle(cx).is_focused(window)
                            && let Some(caret) = field.offset_bounds(field.cursor())
                        {
                            let viewport = scroll.bounds();
                            let offset = scroll.offset();
                            let mut y = offset.y;
                            if caret.top() < viewport.top() {
                                y += viewport.top() - caret.top();
                            } else if caret.bottom() > viewport.bottom() {
                                y -= caret.bottom() - viewport.bottom();
                            }
                            y = y.clamp(-scroll.max_offset().y, px(0.));
                            if y != offset.y {
                                scroll.set_offset(gpui::point(offset.x, y));
                                window.request_animation_frame();
                            }
                        }
                        let first = starts.partition_point(|at| {
                            field
                                .offset_bounds(*at)
                                .is_some_and(|row| row.bottom() < bounds.top())
                        });
                        let rows: Vec<_> = starts
                            .iter()
                            .enumerate()
                            .skip(first)
                            .filter_map(|(index, at)| {
                                field.offset_bounds(*at).map(|row| (index, row))
                            })
                            .take_while(|(_, row)| row.top() < bounds.bottom())
                            .filter(|(_, row)| row.top() >= bounds.top())
                            .collect();
                        for (index, row) in rows {
                            let number = (index + 1).to_string();
                            let run = gpui::TextRun {
                                len: number.len(),
                                font: font.clone(),
                                color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let line = window.text_system().shape_line(
                                number.into(),
                                px(size),
                                &[run],
                                None,
                            );
                            let origin =
                                gpui::point(bounds.right() - line.width - px(6.), row.top());
                            let _ = line.paint(
                                origin,
                                row.size.height,
                                gpui::TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                    },
                )
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(gutter),
            )
            .child(bezel::ui::scroll::Overlay::new(
                "file-source-scrollbar",
                &self.scroll,
                gpui::Axis::Vertical,
            ))
            .into_any_element()
    }
}

impl Focusable for FileView {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus.clone()
    }
}

impl Render for FileView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let size = typography::file_size(cx);
        self.field.update(cx, |field, cx| {
            field.set_metrics(
                bezel::theme::Metrics::from(TextStyle::Body)
                    .scaled(size / TextStyle::Body.painted()),
                cx,
            );
        });
        let markdown = self
            .path
            .extension()
            .is_some_and(|ext| ext == "md" || ext == "markdown");
        let notice = self.error.clone().or_else(|| self.changed.then(|| "File changed on disk. Reload discards your edits; overwrite saves your version.".into()));
        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("FileEditor")
            .track_focus(&self.focus)
            .on_action(|_: &IncreaseTextSize, _, cx| typography::zoom_file(1., cx))
            .on_action(|_: &DecreaseTextSize, _, cx| typography::zoom_file(-1., cx))
            .on_action(|_: &ResetTextSize, _, cx| typography::reset_file_zoom(cx))
            .when(markdown && self.preview, |panel| {
                panel
                    .on_action(cx.listener(|this, _: &bezel::ui::input::Copy, _, cx| {
                        let Some(selection) = this.preview_selection else {
                            return;
                        };
                        let doc = markdown::parse_with(
                            this.field.read(cx).content(),
                            &markdown::Marks::of(cx),
                        );
                        let text = markdown::selectable::copied(&doc, selection);
                        if !text.is_empty() {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                        }
                    }))
                    .on_action(cx.listener(|this, _: &bezel::ui::input::SelectAll, _, cx| {
                        let doc = markdown::parse_with(
                            this.field.read(cx).content(),
                            &markdown::Marks::of(cx),
                        );
                        this.preview_selection = Some(markdown::Selection::all(&doc));
                        cx.notify();
                    }))
            })
            .on_action(cx.listener(|this, _: &Save, _, cx| {
                this.save(false, cx);
            }))
            .when_some(notice, |panel, notice| {
                panel.child(
                    div()
                        .p(px(8.))
                        .text_style(TextStyle::Caption)
                        .text_color(theme.text_muted)
                        .child(notice)
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .id("file-reload")
                                        .cursor_pointer()
                                        .child("Reload")
                                        .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                                )
                                .when(self.ready, |row| {
                                    row.child(
                                        div()
                                            .id("file-overwrite")
                                            .cursor_pointer()
                                            .child("Overwrite")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save(true, cx);
                                            })),
                                    )
                                }),
                        ),
                )
            })
            .when(self.loading, |panel| {
                panel.child(div().p(px(16.)).child("Loading file…"))
            })
            .when(self.ready, |panel| {
                if markdown && self.preview {
                    panel.child(
                        div()
                            .id("file-preview")
                            .cursor(gpui::CursorStyle::IBeam)
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&this.focus, cx);
                                    this.preview_selection = this
                                        .preview_layouts
                                        .hit(event.position)
                                        .map(markdown::Selection::at);
                                    this.preview_dragging = this.preview_selection.is_some();
                                    cx.notify();
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                |this, event: &gpui::MouseMoveEvent, _, cx| {
                                    if this.preview_dragging
                                        && let Some(cursor) =
                                            this.preview_layouts.hit(event.position)
                                        && let Some(selection) = this.preview_selection
                                    {
                                        this.preview_selection = Some(selection.extend_to(cursor));
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, _| this.preview_dragging = false),
                            )
                            .on_mouse_up_out(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, _| this.preview_dragging = false),
                            )
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(16.))
                            .child(markdown::render::render_with(
                                &markdown::parse_with(
                                    self.field.read(cx).content(),
                                    &markdown::Marks::of(cx),
                                ),
                                markdown::render::Editing {
                                    selection: self.preview_selection,
                                    layouts: Some(&self.preview_layouts),
                                    caret_on: false,
                                    typography: Some(
                                        markdown::Typography::of(cx)
                                            .scaled(size / bezel::theme::base_text_size()),
                                    ),
                                    ..Default::default()
                                },
                                window,
                                cx,
                            )),
                    )
                } else {
                    panel.child(self.source_view(&theme, cx))
                }
            })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/file.rs"]
mod tests;
