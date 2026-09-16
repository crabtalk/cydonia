//! A session's persistent Review, terminal, and file tabs.

mod persistence;

use super::{
    changes::Changes,
    file::FileView,
    files::Files,
    terminal::{DirectoryChanged, Exited, Terminal},
};
use crate::view::root::{Cydonia, Pane, ToggleChanges};
use bezel::{
    gpui::{
        self, AnyElement, Axis, Context, DragMoveEvent, Empty, Entity, Focusable, Render,
        Subscription, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        menu::{self, Cursor, Hit, Item},
        popover,
        tooltip::Tooltip,
    },
};
use std::path::PathBuf;

gpui::actions!(
    session_panel,
    [OpenFile, NewTerminal, CloseTab, ToggleFiles]
);

struct FilesResize;

enum Content {
    Review(Entity<Changes>),
    Terminal(Entity<Terminal>),
    File(Entity<FileView>),
}
struct Tab {
    id: usize,
    content: Content,
    _subscriptions: Vec<Subscription>,
}

pub struct Panel {
    cwd: PathBuf,
    project_root: PathBuf,
    files: Option<Entity<Files>>,
    files_open: bool,
    files_width: f32,
    files_subscription: Option<Subscription>,
    focus: gpui::FocusHandle,
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_id: usize,
    menu: bool,
    cursor: Cursor,
    closing: Option<usize>,
    focus_pending: bool,
    restore_pending: Option<persistence::SavedPanel>,
}

impl Panel {
    pub fn new(cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            project_root: cwd.clone(),
            files: None,
            files_open: false,
            files_width: 220.,
            files_subscription: None,
            cwd,
            focus: cx.focus_handle(),
            tabs: Vec::new(),
            active: None,
            next_id: 0,
            menu: false,
            cursor: Cursor::default(),
            closing: None,
            focus_pending: false,
            restore_pending: None,
        }
    }

    fn push(
        &mut self,
        content: Content,
        subscriptions: Vec<Subscription>,
        cx: &mut Context<Self>,
    ) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            content,
            _subscriptions: subscriptions,
        });
        self.active = Some(id);
        cx.notify();
        id
    }

    pub fn review(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| matches!(tab.content, Content::Review(_)))
        {
            self.active = Some(tab.id);
            cx.notify();
            return;
        }
        let review = cx.new(|cx| Changes::new(self.cwd.clone(), cx));
        let open = cx.subscribe(&review, |this, _, event: &super::changes::OpenFile, cx| {
            this.open_file(event.0.clone(), cx);
        });
        let watch = cx.observe(&review, |_, _, cx| cx.notify());
        self.push(Content::Review(review), vec![open, watch], cx);
    }

    fn terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = cx.new(|cx| Terminal::new(&self.cwd, cx));
        let id = self.next_id;
        let exit = cx.subscribe_in(&terminal, window, move |this, _, _: &Exited, window, cx| {
            let focused = this.tabs.iter().find(|tab| tab.id == id).is_some_and(|tab| {
                matches!(&tab.content, Content::Terminal(terminal) if terminal.focus_handle(cx).is_focused(window))
            });
            this.remove(id, cx);
            if focused { this.focus(window, cx); }
        });
        let directory = cx.subscribe(&terminal, |_, _, _: &DirectoryChanged, cx| cx.notify());
        window.focus(&terminal.focus_handle(cx), cx);
        self.push(Content::Terminal(terminal), vec![exit, directory], cx);
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter().find(|tab| Some(tab.id) == self.active) {
            match &tab.content {
                Content::Terminal(terminal) => window.focus(&terminal.focus_handle(cx), cx),
                Content::File(file) => window.focus(&file.focus_handle(cx), cx),
                Content::Review(_) => window.focus(&self.focus, cx),
            }
        } else {
            window.focus(&self.focus, cx);
        }
    }

    fn files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files.is_none() {
            let files = cx.new(|cx| Files::new(self.project_root.clone(), cx));
            self.files_subscription = Some(cx.subscribe(
                &files,
                |this, _, event: &super::files::Open, cx| {
                    this.open_file(event.0.clone(), cx);
                },
            ));
            self.files = Some(files);
        }
        self.files_open = true;
        if let Some(files) = &self.files {
            window.focus(&files.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.focus_pending = true;
        let path = path.canonicalize().unwrap_or(path);
        if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| matches!(&tab.content, Content::File(file) if file.read(cx).path == path))
        {
            self.active = Some(tab.id);
            cx.notify();
            return;
        }
        let file = cx.new(|cx| {
            let mut file = FileView::new(path, cx);
            file.root = self.project_root.clone();
            file
        });
        let watch = cx.observe(&file, |_, _, cx| cx.notify());
        self.push(Content::File(file), vec![watch], cx);
    }

    fn remove(&mut self, id: usize, cx: &mut Context<Self>) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        self.tabs.remove(index);
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.id);
        }
        if self.closing == Some(id) {
            self.closing = None;
        }
        cx.notify();
    }

    fn close(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.iter().any(|tab| {
            tab.id == id && matches!(&tab.content, Content::File(file) if file.read(cx).dirty(cx))
        }) {
            self.closing = Some(id);
            self.active = Some(id);
            cx.notify();
        } else {
            self.remove(id, cx);
            self.focus(window, cx);
        }
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = false;
        match index {
            0 => self.review(cx),
            1 => self.terminal(window, cx),
            2 => self.files(window, cx),
            _ => {}
        }
        cx.notify();
    }

    fn items(window: &Window) -> Vec<Item> {
        vec![
            Item::action("Review")
                .with_icon(icons::development::GitCompare)
                .with_shortcut(&crate::view::root::OpenReview, window),
            Item::action("Terminal").with_icon(icons::development::Terminal),
            Item::action("Files")
                .with_icon(icons::files::Folder)
                .with_shortcut(&crate::view::root::OpenFiles, window),
        ]
    }
}

impl Render for Panel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.restore_tabs(window, cx);
        if self.focus_pending {
            self.focus_pending = false;
            self.focus(window, cx);
        }
        let theme = Theme::of(cx).clone();
        let items = Self::items(window);
        let rows = items.clone();
        let popup = self.menu.then(|| {
            menu::card(
                &theme,
                "panel-menu",
                &items,
                &self.cursor,
                cx,
                move |this, hit, window, cx| {
                    match hit {
                        Hit::Point(path) => {
                            this.cursor.point_at(&rows, &path);
                        }
                        Hit::Choose(path) => {
                            if let Some(index) = path.first() {
                                this.choose(*index, window, cx);
                            }
                        }
                        Hit::Dismiss => this.menu = false,
                    }
                    cx.notify();
                },
            )
        });
        let active_file = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active)
            .and_then(|tab| {
                if let Content::File(file) = &tab.content {
                    Some(file.clone())
                } else {
                    None
                }
            });
        if self.files_open
            && let Some(files) = &self.files
        {
            let selected = active_file.as_ref().map(|file| file.read(cx).path.clone());
            files.update(cx, |files, cx| files.reveal(selected, cx));
        }
        let status = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active)
            .map(|tab| match &tab.content {
                Content::File(file) => {
                    file.update(cx, |file, cx| file.status_bar(self.files_open, window, cx))
                }
                Content::Review(review) => {
                    review.update(cx, |review, cx| review.status_bar(self.files_open, cx))
                }
                Content::Terminal(terminal) => {
                    let directory = &terminal.read(cx).directory;
                    super::status::bar(&theme)
                        .child(super::status::path(
                            directory,
                            directory.display().to_string(),
                        ))
                        .child(super::status::files_toggle(self.files_open, &theme))
                        .into_any_element()
                }
            })
            .unwrap_or_else(|| {
                super::status::bar(&theme)
                    .child(super::status::path(
                        &self.project_root,
                        self.project_root.display().to_string(),
                    ))
                    .child(super::status::files_toggle(self.files_open, &theme))
                    .into_any_element()
            });
        let body: AnyElement = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active)
            .map(|tab| match &tab.content {
                Content::Review(review) => review.clone().into_any_element(),
                Content::Terminal(terminal) => terminal.clone().into_any_element(),
                Content::File(file) => file.clone().into_any_element(),
            })
            .unwrap_or_else(|| {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(6.))
                    .p(px(24.))
                    .children(
                        [
                            ("Review", icons::development::GitCompare),
                            ("Terminal", icons::development::Terminal),
                            ("Files", icons::files::Folder),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (label, icon))| {
                            div()
                                .id(("panel-launch", index))
                                .h(px(42.))
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .px(px(12.))
                                .rounded(px(8.))
                                .bg(theme.element_hover)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose(index, window, cx)
                                }))
                                .child(icons::icon(icon).size(px(16.)).text_color(theme.text_muted))
                                .child(label)
                        }),
                    )
                    .into_any_element()
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(crate::view::root::content_bg(&theme))
            .key_context("SessionPanel")
            .track_focus(&self.focus)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.focus.contains_focused(window, cx) {
                        this.focus(window, cx);
                    }
                }),
            )
            .on_action(
                cx.listener(|this, action: &super::files::ToggleFilter, window, cx| {
                    if !this.files_open {
                        this.files(window, cx);
                    }
                    if let Some(files) = &this.files {
                        files.update(cx, |files, cx| files.toggle_filter(action, window, cx));
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &ToggleFiles, window, cx| {
                if this.files_open {
                    this.files_open = false;
                    this.focus(window, cx);
                    cx.notify();
                } else {
                    this.files(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFile, window, cx| this.files(window, cx)))
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.terminal(window, cx)))
            .on_action(
                cx.listener(|this, _: &crate::view::menubar::CloseWindow, window, cx| {
                    if let Some(id) = this.active {
                        this.close(id, window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                if let Some(id) = this.active {
                    this.close(id, window, cx);
                }
            }))
            .child(
                div()
                    .h(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(8.))
                    .child(
                        div()
                            .id("panel-tabs")
                            .min_w_0()
                            .flex()
                            .gap(px(4.))
                            .overflow_x_scroll()
                            .children(self.tabs.iter().map(|tab| {
                                let id = tab.id;
                                let icon = match &tab.content {
                                    Content::Review(_) => icons::development::GitCompare,
                                    Content::Terminal(_) => icons::development::Terminal,
                                    Content::File(_) => icons::files::File,
                                };
                                let (label, path) = match &tab.content {
                                    Content::Review(_) => {
                                        ("Review".to_string(), "Review".to_string())
                                    }
                                    Content::Terminal(terminal) => {
                                        let path = &terminal.read(cx).directory;
                                        (
                                            path.file_name()
                                                .unwrap_or(path.as_os_str())
                                                .to_string_lossy()
                                                .into_owned(),
                                            path.display().to_string(),
                                        )
                                    }
                                    Content::File(file) => {
                                        let file = file.read(cx);
                                        (
                                            format!(
                                                "{}{}",
                                                file.path
                                                    .file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                                if file.dirty(cx) { " •" } else { "" }
                                            ),
                                            file.path.display().to_string(),
                                        )
                                    }
                                };
                                div()
                                    .id(("panel-tab", id))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .h(px(28.))
                                    .max_w(px(180.))
                                    .px(px(10.))
                                    .rounded(px(8.))
                                    .cursor_pointer()
                                    .text_style(TextStyle::Caption)
                                    .when(self.active == Some(id), |tab| {
                                        tab.bg(theme.element_hover)
                                    })
                                    .tooltip(move |window, cx| {
                                        Tooltip::text(path.clone(), window, cx)
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.active = Some(id);
                                        this.focus(window, cx);
                                        cx.notify();
                                    }))
                                    .child(
                                        icons::icon(icon)
                                            .size(px(14.))
                                            .flex_none()
                                            .text_color(theme.text_muted),
                                    )
                                    .child(div().min_w_0().truncate().child(label))
                                    .child(
                                        div()
                                            .id(("panel-tab-close", id))
                                            .flex_none()
                                            .size(px(16.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.close(id, window, cx);
                                            }))
                                            .child(
                                                icons::icon(icons::notifications::X)
                                                    .size(px(12.))
                                                    .text_color(theme.text_muted),
                                            ),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .child(
                                div()
                                    .id("panel-add")
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .tooltip(|window, cx| Tooltip::text("New tab", window, cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.menu = !this.menu;
                                        this.cursor.clear();
                                        cx.notify();
                                    }))
                                    .child(
                                        icons::icon(icons::math::Plus)
                                            .size(px(16.))
                                            .text_color(theme.text_muted),
                                    ),
                            )
                            .children(popup.map(|popup| {
                                popover::anchored_menu_below(
                                    "panel-menu",
                                    popup.into_any_element(),
                                    None,
                                )
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("panel-hide")
                            .flex_none()
                            .size(px(24.))
                            .cursor_pointer()
                            .tooltip(|window, cx| Tooltip::text("Hide right panel", window, cx))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleChanges), cx)
                            })
                            .child(
                                icons::icon(icons::layout::PanelLeftClose)
                                    .size(px(16.))
                                    .text_color(theme.text_muted),
                            ),
                    ),
            )
            .when_some(self.closing, |panel, id| {
                panel.child(
                    div()
                        .p(px(8.))
                        .text_style(TextStyle::Caption)
                        .child("Save changes before closing?")
                        .child(
                            div()
                                .flex()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .id("file-close-save")
                                        .cursor_pointer()
                                        .child("Save and close")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            let file =
                                                this.tabs.iter().find(|tab| tab.id == id).and_then(
                                                    |tab| match &tab.content {
                                                        Content::File(file) => Some(file.clone()),
                                                        _ => None,
                                                    },
                                                );
                                            if file.is_some_and(|file| {
                                                file.update(cx, |file, cx| file.save(false, cx))
                                            }) {
                                                this.remove(id, cx);
                                                this.focus(window, cx);
                                            }
                                        })),
                                )
                                .child(
                                    div()
                                        .id("file-close-discard")
                                        .cursor_pointer()
                                        .child("Discard")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.remove(id, cx);
                                            this.focus(window, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .id("file-close-cancel")
                                        .cursor_pointer()
                                        .child("Cancel")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.closing = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .on_drag_move(
                        cx.listener(|this, event: &DragMoveEvent<FilesResize>, _, cx| {
                            let available = f32::from(event.bounds.size.width);
                            let min = 80_f32.min(available / 2.);
                            this.files_width =
                                f32::from(event.bounds.right() - event.event.position.x)
                                    .clamp(min, (available * 0.8).max(min));
                            cx.notify();
                        }),
                    )
                    .when(!(self.files_open && self.tabs.is_empty()), |row| {
                        row.child(div().flex_1().min_w_0().child(body))
                    })
                    .when(self.files_open, |row| {
                        row.children(self.files.clone().map(|files| {
                            div()
                                .when(self.tabs.is_empty(), |tree| tree.flex_1().w_full())
                                .when(!self.tabs.is_empty(), |tree| {
                                    tree.w(px(self.files_width))
                                        .max_w(gpui::relative(0.8))
                                        .flex_none()
                                })
                                .relative()
                                .child(files)
                                .when(!self.tabs.is_empty(), |tree| {
                                    tree.child(
                                        crate::view::component::divider::divider(
                                            &theme,
                                            Axis::Horizontal,
                                        )
                                        .id("files-split")
                                        .absolute()
                                        .top_0()
                                        .left(px(-crate::view::component::divider::HIT / 2.))
                                        .on_drag(FilesResize, |_, _, _, cx| cx.new(|_| Empty)),
                                    )
                                })
                        }))
                    }),
            )
            .child(status)
    }
}

impl Cydonia {
    pub(crate) fn show_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.showing(cx) == Some(Pane::Chat) {
            self.changes_open = true;
            self.sync_changes(cx);
            if let Some(panel) = self.changes.clone() {
                panel.update(cx, |panel, cx| panel.files(window, cx));
            }
            cx.notify();
        }
    }

    pub(crate) fn show_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.showing(cx) == Some(Pane::Chat) {
            self.changes_open = true;
            self.sync_changes(cx);
            if let Some(panel) = self.changes.clone() {
                panel.update(cx, |panel, cx| {
                    panel.restore_tabs(window, cx);
                    panel.review(cx);
                    panel.focus(window, cx);
                });
            }
            cx.notify();
        }
    }

    pub(crate) fn toggle_changes(
        &mut self,
        _: &ToggleChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.showing(cx) == Some(Pane::Chat) {
            self.changes_open = !self.changes_open;
            if !self.changes_open {
                window.focus(&self.composer_focus_handle(cx), cx);
            } else {
                self.sync_changes(cx);
                if let Some(panel) = self.changes.clone() {
                    panel.update(cx, |panel, cx| panel.focus(window, cx));
                }
            }
            cx.notify();
        }
    }

    pub(crate) fn sync_changes(&mut self, cx: &mut Context<Self>) {
        let workspace = self.workspace.read(cx);
        self.right_panels
            .retain(|id, _| workspace.session(*id).is_some());
        let session = (self.changes_open && self.showing(cx) == Some(Pane::Chat))
            .then(|| {
                workspace
                    .active_session()
                    .map(|chat| (chat.id, chat.cwd.clone(), chat.record.clone()))
            })
            .flatten();
        let project_root = workspace
            .active_project()
            .map(|project| project.path.clone());
        self.changes = session.map(|(id, cwd, record)| {
            self.right_panels
                .entry(id)
                .or_insert_with(|| {
                    cx.new(|cx| {
                        let saved = record
                            .as_deref()
                            .and_then(|record| persistence::saved_panel(&cwd, record));
                        let mut panel = Panel::new(cwd, cx);
                        panel.restore_pending = saved;
                        if let Some(root) = project_root {
                            panel.project_root = root.canonicalize().unwrap_or(root);
                        }
                        panel
                    })
                })
                .clone()
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/panel.rs"]
mod tests;
