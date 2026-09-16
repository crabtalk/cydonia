//! Small text-file buffers with explicit saves and external-change detection.

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
gpui::actions!(file_editor, [Save]);

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
    scroll: gpui::ScrollHandle,
    reveal: Rc<Cell<bool>>,
    _watch: Subscription,
    _poll: Task<()>,
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
            path,
            field,
            saved: String::new(),
            ready: false,
            loading: true,
            error: None,
            changed: false,
            preview: true,
            scroll: gpui::ScrollHandle::new(),
            reveal: Rc::new(Cell::new(false)),
            _watch: watch,
            _poll: poll,
        }
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
                            if !this.preview {
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
    fn source_view(&self, theme: &Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        let starts = line_starts(self.field.read(cx).content());
        let size = TextStyle::Body.painted();
        let gutter = px(starts.len().to_string().len() as f32 * size * 0.65 + 20.);
        let field = self.field.clone();
        let scroll = self.scroll.clone();
        let reveal = self.reveal.clone();
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
                        if reveal.replace(false) && field.focus_handle(cx).is_focused(window) {
                            if let Some(caret) = field.offset_bounds(field.cursor()) {
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
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(16.))
                            .child(markdown::render::markdown(
                                &self.field.read(cx).content().clone(),
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
