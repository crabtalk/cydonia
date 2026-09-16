//! Session Git changes panel with background polling while visible.

use crate::model::git::{
    self, Change, Repository,
    preview::{Kind, Preview},
};
use bezel::ui::scroll as scrollbars;
use bezel::{
    gpui::{
        self, AnyElement, ClipboardItem, Context, HighlightStyle, Hsla, Pixels, Render,
        SharedString, StyledText, Task, TextRun, UniformListScrollHandle, Window, canvas, div,
        font, point, prelude::*, px, uniform_list,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons::{self, Icon},
        tooltip::Tooltip,
    },
};
use std::{collections::HashSet, ops::Range, path::PathBuf, sync::Arc, time::Duration};

pub struct OpenFile(pub PathBuf);
impl gpui::EventEmitter<OpenFile> for Changes {}

pub struct Changes {
    pub cwd: PathBuf,
    repository: Option<Repository>,
    selected: Option<Change>,
    preview: Arc<Preview>,
    rows: Vec<usize>,
    collapsed: HashSet<String>,
    wrapped: Vec<Option<Vec<Range<usize>>>>,
    visual_rows: Vec<(usize, Range<usize>)>,
    font_key: Option<(SharedString, u32)>,
    wrap_width: Pixels,
    number_width: Pixels,
    error: Option<String>,
    loading: bool,
    ready: bool,
    files_scroll: UniformListScrollHandle,
    diff_scroll: UniformListScrollHandle,
    load: Option<Task<()>>,
    _poll: Task<()>,
}

impl Changes {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                if this
                    .update(cx, |this, cx| {
                        if !this.loading {
                            this.refresh(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut this = Self {
            cwd,
            repository: None,
            selected: None,
            preview: Arc::default(),
            rows: Vec::new(),
            collapsed: HashSet::new(),
            wrapped: Vec::new(),
            visual_rows: Vec::new(),
            font_key: None,
            wrap_width: px(0.),
            number_width: px(0.),
            error: None,
            loading: false,
            ready: false,
            files_scroll: UniformListScrollHandle::new(),
            diff_scroll: UniformListScrollHandle::new(),
            load: None,
            _poll: poll,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let cwd = self.cwd.clone();
        let selected = self.selected.clone();
        let previous = self.preview.clone();
        self.loading = true;
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let repository = git::status(&cwd)?;
                    let selected = repository.as_ref().and_then(|repo| {
                        selected
                            .and_then(|selected| {
                                repo.files
                                    .iter()
                                    .find(|file| {
                                        file.path == selected.path && file.area == selected.area
                                    })
                                    .cloned()
                            })
                            .or_else(|| repo.files.first().cloned())
                    });
                    let preview = match (&repository, &selected) {
                        (Some(repo), Some(file)) => match git::diff(&repo.root, file) {
                            Ok(patch) => Preview::load(&repo.root, file, patch, previous),
                            Err(error) => Preview::plain(format!("Could not read diff: {error}")),
                        },
                        _ => Arc::default(),
                    };
                    anyhow::Ok((repository, selected, preview))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                this.ready = true;
                match result {
                    Ok((repository, selected, preview)) => {
                        if this.selected != selected {
                            this.diff_scroll = UniformListScrollHandle::new();
                            this.collapsed.clear();
                        }
                        if !Arc::ptr_eq(&this.preview, &preview) {
                            this.wrapped.clear();
                        }
                        this.repository = repository;
                        this.selected = selected;
                        this.preview = preview;
                        this.rows = this.preview.visible_rows(&this.collapsed);
                        this.error = None;
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                        this.repository = None;
                        this.selected = None;
                        this.preview = Arc::default();
                        this.rows.clear();
                        this.wrapped.clear();
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn file_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx);
        let file = self.repository.as_ref().unwrap().files[ix].clone();
        let selected = self.selected.as_ref() == Some(&file);
        let label = match &file.original {
            Some(original) => format!("{} → {}", original.display(), file.path.display()),
            None => file.path.display().to_string(),
        };
        div()
            .id(("git-file", ix))
            .h(px(28.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(if selected {
                theme.accent.opacity(0.12)
            } else {
                gpui::transparent_black()
            })
            .hover(|style| style.bg(theme.surface_raised))
            .cursor_pointer()
            .text_style(TextStyle::Caption)
            .child(
                div()
                    .w(px(14.))
                    .flex_none()
                    .text_color(match file.status {
                        'D' => theme.danger,
                        'A' | '?' => theme.success,
                        _ => theme.text_muted,
                    })
                    .child(file.status.to_string()),
            )
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(
                div()
                    .flex_none()
                    .text_color(theme.text_faint)
                    .child(file.area.label()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.selected.as_ref() != Some(&file) {
                    this.selected = Some(file.clone());
                    this.preview = Arc::default();
                    this.rows.clear();
                    this.wrapped.clear();
                    this.collapsed.clear();
                    this.diff_scroll = UniformListScrollHandle::new();
                    this.refresh(cx);
                }
            }))
            .into_any_element()
    }

    fn diff_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx);
        let (ix, segment) = &self.visual_rows[ix];
        let ix = *ix;
        let first = segment.start == 0;
        let line = &self.preview.lines[ix];
        if line.is_hunk() {
            return self.hunk_row(ix, cx);
        }
        let (wash, mark, sign) = match line.kind {
            Kind::Added => (diff_wash(theme, theme.diff_add), theme.diff_add, "+"),
            Kind::Removed => (diff_wash(theme, theme.diff_del), theme.diff_del, "−"),
            Kind::Hunk => (theme.bg.blend(theme.diff_hunk_bg), theme.text_faint, ""),
            _ => (theme.bg, theme.text_faint, ""),
        };
        let number = |value: Option<usize>| {
            div()
                .w(self.number_width)
                .flex_none()
                .text_align(gpui::TextAlign::Right)
                .text_color(theme.text_faint)
                .child(value.map(|value| value.to_string()).unwrap_or_default())
        };
        let text = StyledText::new(line.text[segment.clone()].to_owned()).with_highlights(
            line.spans.iter().filter_map(|(range, kind)| {
                clipped_range(range, segment).map(|range| {
                    (
                        range,
                        HighlightStyle {
                            color: Some(theme.syntax.color(*kind)),
                            ..Default::default()
                        },
                    )
                })
            }),
        );
        div()
            .h(px(20.))
            .w_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .pr(px(4.))
            .bg(wash)
            .text_color(if matches!(line.kind, Kind::Hunk | Kind::Meta) {
                theme.text_muted
            } else {
                theme.text
            })
            .font_family(theme.font_mono.clone())
            .text_style(TextStyle::Callout)
            .line_height(px(20.))
            .whitespace_nowrap()
            .child(div().w(px(2.)).h_full().flex_none().bg(
                if matches!(line.kind, Kind::Added | Kind::Removed) {
                    mark.opacity(0.45)
                } else {
                    gpui::transparent_black()
                },
            ))
            .child(number(line.old.filter(|_| first)))
            .child(number(line.new.filter(|_| first)))
            .child(
                div()
                    .w(px(10.))
                    .flex_none()
                    .text_color(mark)
                    .child(if first { sign } else { "" }),
            )
            .child(div().flex_1().min_w_0().overflow_hidden().child(text))
            .into_any_element()
    }
}

impl Changes {
    pub fn status_bar(&mut self, files_open: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let path = match (&self.repository, &self.selected) {
            (Some(repo), Some(file)) => repo.root.join(&file.path),
            (Some(repo), None) => repo.root.clone(),
            _ => self.cwd.clone(),
        };
        let label = match &self.selected {
            Some(file) => format!("{} · {}", file.area.label(), path.display()),
            None => path.display().to_string(),
        };
        super::status::bar(&theme)
            .child(super::status::path(&path, label))
            .when(self.selected.is_some(), |bar| {
                bar.child(
                    tool(&theme, "git-open-file", "Open file", icons::files::Folder).on_click(
                        cx.listener(|this, _, _, cx| {
                            if let (Some(repo), Some(file)) = (&this.repository, &this.selected) {
                                cx.emit(OpenFile(repo.root.join(&file.path)));
                            }
                        }),
                    ),
                )
                .child(
                    tool(&theme, "git-copy", "Copy diff", icons::text::Copy).on_click(cx.listener(
                        |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                this.preview.patch.clone(),
                            ));
                        },
                    )),
                )
            })
            .child(super::status::files_toggle(files_open, &theme))
            .into_any_element()
    }
}

impl Render for Changes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        self.measure(window, cx);
        let count = self.repository.as_ref().map_or(0, |repo| repo.files.len());
        let message = self.error.clone().or_else(|| {
            if !self.ready {
                Some("Loading review…".into())
            } else if self.repository.is_none() {
                Some("This directory is not in a Git repository.".into())
            } else if count == 0 {
                Some("Working tree clean".into())
            } else {
                None
            }
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(crate::view::root::content_bg(&theme))
            .when_some(message, |panel, message| {
                panel.child(
                    div()
                        .p(px(16.))
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_muted)
                        .child(message),
                )
            })
            .when(count > 0, |panel| {
                panel
                    .child(
                        div()
                            .relative()
                            .h(px((count as f32 * 28.).min(168.)))
                            .flex_none()
                            .child(
                                uniform_list(
                                    "git-files",
                                    count,
                                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                        range.map(|ix| this.file_row(ix, cx)).collect()
                                    }),
                                )
                                .track_scroll(&self.files_scroll)
                                .size_full(),
                            )
                            .child(scrollbars::Overlay::new(
                                "git-files-bar",
                                &self.files_scroll.0.borrow().base_handle,
                                bezel::gpui::Axis::Vertical,
                            )),
                    )
                    .when(self.loading && self.preview.lines.is_empty(), |panel| {
                        panel.child(div().p(px(12.)).child("Loading diff…"))
                    })
                    .child(self.diff_body(cx))
            })
    }
}

fn tool(
    theme: &Theme,
    id: &'static str,
    label: &'static str,
    icon: impl Into<Icon>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(24.))
        .flex_none()
        .rounded(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(theme.element_hover))
        .tooltip(move |window, cx| Tooltip::text(label, window, cx))
        .child(icons::icon(icon).size(px(13.)).text_color(theme.text_muted))
}

/// Composite over an opaque surface to keep code readable with vibrancy.
fn diff_wash(theme: &Theme, tone: Hsla) -> Hsla {
    theme.bg.blend(Hsla {
        s: tone.s * 0.45,
        a: 0.055,
        ..tone
    })
}

/// Intersect source highlighting with a visual continuation and rebase its bytes.
fn clipped_range(source: &Range<usize>, segment: &Range<usize>) -> Option<Range<usize>> {
    let start = source.start.max(segment.start);
    let end = source.end.min(segment.end);
    (start < end).then(|| start - segment.start..end - segment.start)
}

impl Changes {
    /// Wrap with native font metrics into fixed-height rows for virtualization.
    fn measure(&mut self, window: &Window, cx: &Context<Self>) {
        let theme = Theme::of(cx);
        let size = TextStyle::Callout.painted();
        let key = (theme.font_mono.clone(), size.to_bits());
        if self.font_key.as_ref() != Some(&key) {
            self.font_key = Some(key);
            self.wrapped.clear();
        }
        let text_system = window.text_system();
        let run = |len| TextRun {
            len,
            font: font(theme.font_mono.clone()),
            color: theme.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let number = self.preview.lines.iter().fold(1, |number, line| {
            number.max(line.old.unwrap_or(0)).max(line.new.unwrap_or(0))
        });
        let digits = "0".repeat(number.to_string().len().max(2));
        self.number_width = text_system
            .shape_line(digits.clone().into(), px(size), &[run(digits.len())], None)
            .width;
        let viewport = self.diff_scroll.0.borrow().base_handle.bounds().size.width;
        // Use the default width until the bounds observer triggers reflow.
        let viewport = if viewport > px(0.) {
            viewport
        } else {
            px(440.)
        };
        let width = (viewport - self.number_width * 2. - px(32.)).max(px(size));
        if self.wrap_width != width {
            self.wrap_width = width;
            self.wrapped.clear();
        }
        self.wrapped.resize(self.preview.lines.len(), None);
        self.visual_rows.clear();
        for &ix in &self.rows {
            let line = &self.preview.lines[ix];
            let segments = self.wrapped[ix].get_or_insert_with(|| {
                let mut starts = vec![0];
                if !line.is_hunk()
                    && let Ok(shaped) = text_system.shape_text(
                        line.text.clone().into(),
                        px(size),
                        &[run(line.text.len())],
                        Some(width),
                        None,
                    )
                {
                    for shaped in shaped {
                        for boundary in &shaped.wrap_boundaries {
                            starts.push(
                                shaped.unwrapped_layout.runs[boundary.run_ix].glyphs
                                    [boundary.glyph_ix]
                                    .index,
                            );
                        }
                    }
                }
                starts.push(line.text.len());
                starts.windows(2).map(|pair| pair[0]..pair[1]).collect()
            });
            self.visual_rows
                .extend(segments.iter().cloned().map(|range| (ix, range)));
        }
    }

    fn hunk_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx);
        let key = self.preview.lines[ix].text.clone();
        let collapsed = self.collapsed.contains(&key);
        div()
            .id(("git-hunk", ix))
            .h(px(20.))
            .w_full()
            .relative()
            .overflow_hidden()
            .bg(theme.bg.blend(theme.diff_hunk_bg))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.collapsed.remove(&key) {
                    this.collapsed.insert(key.clone());
                }
                this.rows = this.preview.visible_rows(&this.collapsed);
                cx.notify();
            }))
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(4.))
                    .text_style(TextStyle::Subheadline)
                    .text_color(theme.text_muted)
                    .child(
                        icons::icon(if collapsed {
                            icons::arrows::ChevronRight
                        } else {
                            icons::arrows::ChevronDown
                        })
                        .size(px(12.))
                        .flex_none()
                        .text_color(theme.text_muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(self.preview.hunk_label(ix)),
                    ),
            )
            .into_any_element()
    }

    fn diff_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx);
        if self.rows.is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .bg(theme.bg)
                .when(!self.loading, |body| {
                    body.child(
                        div()
                            .px(px(8.))
                            .py(px(12.))
                            .text_style(TextStyle::Callout)
                            .text_color(theme.text_muted)
                            .child(self.preview.summary()),
                    )
                })
                .into_any_element();
        }
        let handle = self.diff_scroll.0.borrow().base_handle.clone();
        let before = (handle.bounds(), handle.max_offset(), handle.offset());
        let owner = cx.entity().downgrade();
        let sync = handle.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(theme.bg)
            .child(
                uniform_list(
                    "git-diff",
                    self.visual_rows.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|ix| this.diff_row(ix, cx)).collect()
                    }),
                )
                .size_full()
                .track_scroll(&self.diff_scroll),
            )
            .child(scrollbars::Overlay::new(
                "git-diff-vertical",
                &handle,
                bezel::gpui::Axis::Vertical,
            ))
            .child(
                canvas(
                    move |_, window, _| {
                        let max = sync.max_offset();
                        let offset = sync.offset();
                        let clamped = point(px(0.), offset.y.clamp(-max.y.max(px(0.)), px(0.)));
                        if clamped != offset {
                            sync.set_offset(clamped);
                        }
                        let after = (sync.bounds(), sync.max_offset(), sync.offset());
                        if before != after {
                            let owner = owner.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = owner.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/git_changes.rs"]
mod tests;
