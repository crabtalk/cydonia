//! Project-root file navigation, loaded off the UI thread.

use bezel::{
    gpui::{
        self, Context, Entity, EventEmitter, Focusable, Render, Subscription, Task,
        UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::{FieldEvent, TextField},
    },
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Open(pub PathBuf);
#[derive(Clone)]
struct Entry {
    path: PathBuf,
    directory: bool,
    depth: usize,
}

fn scan(
    root: &Path,
    expanded: &HashSet<PathBuf>,
    query: &str,
) -> anyhow::Result<(Vec<Entry>, bool)> {
    fn visit(
        path: &Path,
        expanded: &HashSet<PathBuf>,
        query: &str,
        depth: usize,
        remaining: &mut usize,
        rows: &mut Vec<Entry>,
    ) -> anyhow::Result<()> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            if *remaining == 0 {
                break;
            }
            *remaining -= 1;
            let entry = entry?;
            entries.push((entry.file_type()?.is_dir(), entry.path()));
        }
        entries.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.file_name().cmp(&b.1.file_name()))
        });
        for (directory, path) in entries {
            let matches = query.is_empty()
                || path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(query);
            if matches {
                rows.push(Entry {
                    path: path.clone(),
                    directory,
                    depth: if query.is_empty() { depth } else { 0 },
                });
            }
            if directory
                && *remaining > 0
                && depth < 64
                && (!query.is_empty() || expanded.contains(&path))
            {
                visit(&path, expanded, query, depth + 1, remaining, rows)?;
            }
        }
        Ok(())
    }
    let mut rows = Vec::new();
    let mut remaining = 20_000;
    visit(
        root,
        expanded,
        &query.to_lowercase(),
        0,
        &mut remaining,
        &mut rows,
    )?;
    Ok((rows, remaining == 0))
}

pub struct Files {
    root: PathBuf,
    filter: Entity<TextField>,
    expanded: HashSet<PathBuf>,
    entries: Vec<Entry>,
    pub selected: Option<PathBuf>,
    error: Option<String>,
    loading: bool,
    truncated: bool,
    scroll: UniformListScrollHandle,
    _filter: Subscription,
    load: Option<Task<()>>,
    _poll: Task<()>,
}
impl EventEmitter<Open> for Files {}

impl Files {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| TextField::new(cx).with_placeholder("Filter files…"));
        let watch = cx.subscribe(&filter, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed) {
                this.refresh(cx);
            }
        });
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
            root,
            filter,
            expanded: HashSet::new(),
            entries: Vec::new(),
            selected: None,
            error: None,
            loading: false,
            truncated: false,
            scroll: UniformListScrollHandle::new(),
            _filter: watch,
            load: None,
            _poll: poll,
        };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let expanded = self.expanded.clone();
        let query = self.filter.read(cx).content().to_string();
        self.loading = true;
        self.load = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { scan(&root, &expanded, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok((entries, truncated)) => {
                        this.entries = entries;
                        this.truncated = truncated;
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn reveal(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.selected == path {
            return;
        }
        if let Some(path) = &path {
            let mut parent = path.parent();
            while let Some(dir) = parent {
                if dir == self.root || !dir.starts_with(&self.root) {
                    break;
                }
                self.expanded.insert(dir.to_path_buf());
                parent = dir.parent();
            }
        }
        self.selected = path;
        self.refresh(cx);
    }
}
impl Focusable for Files {
    fn focus_handle(&self, cx: &gpui::App) -> gpui::FocusHandle {
        self.filter.focus_handle(cx)
    }
}
impl Render for Files {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let searching = !self.filter.read(cx).content().is_empty();
        div()
            .size_full()
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(6.))
            .child(self.filter.clone())
            .when_some(self.error.clone(), |tree, error| {
                tree.child(div().text_style(TextStyle::Caption).child(error))
            })
            .when(self.truncated, |tree| {
                tree.child(
                    div()
                        .text_style(TextStyle::Caption)
                        .child("Showing the first 20,000 entries."),
                )
            })
            .when(self.entries.is_empty(), |tree| {
                tree.child(
                    div()
                        .p(px(8.))
                        .text_color(theme.text_muted)
                        .child(if self.loading {
                            "Loading files…"
                        } else {
                            "No files found"
                        }),
                )
            })
            .child(
                uniform_list(
                    "project-files",
                    self.entries.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let entry = this.entries[index].clone();
                                let label = if searching {
                                    entry
                                        .path
                                        .strip_prefix(&this.root)
                                        .unwrap_or(&entry.path)
                                        .display()
                                        .to_string()
                                } else {
                                    entry
                                        .path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned()
                                };
                                let icon = if entry.directory {
                                    if this.expanded.contains(&entry.path) {
                                        icons::files::FolderOpen
                                    } else {
                                        icons::files::Folder
                                    }
                                } else {
                                    icons::files::File
                                };
                                let selected = this.selected.as_ref() == Some(&entry.path);
                                div()
                                    .id(("project-file", index))
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .pl(px(6. + entry.depth as f32 * 12.))
                                    .pr(px(6.))
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .text_style(TextStyle::Body)
                                    .when(selected, |row| row.bg(theme.element_hover))
                                    .hover(|row| row.bg(theme.element_hover))
                                    .child(
                                        icons::icon(icon)
                                            .size(px(14.))
                                            .flex_none()
                                            .text_color(theme.text_muted),
                                    )
                                    .child(div().min_w_0().truncate().child(label))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if entry.directory {
                                            if !this.expanded.remove(&entry.path) {
                                                this.expanded.insert(entry.path.clone());
                                            }
                                            this.filter.update(cx, |filter, cx| {
                                                if !filter.content().is_empty() {
                                                    filter.set_content("", cx);
                                                }
                                            });
                                            this.refresh(cx);
                                        } else {
                                            cx.emit(Open(entry.path.clone()));
                                        }
                                    }))
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0()
                .track_scroll(&self.scroll),
            )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/files.rs"]
mod tests;
