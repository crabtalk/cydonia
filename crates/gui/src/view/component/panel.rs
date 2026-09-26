//! A session's persistent Review, terminal, and file tabs.

mod persistence;

#[cfg(feature = "desktop")]
use super::terminal::{DirectoryChanged, Exited, Terminal};
use super::{changes::Changes, file::FileView, files::Files};
use crate::view::chrome;
use crate::view::leaf::Pane;
use crate::view::root::{Cydonia, ToggleChanges};
use bezel::{
    gpui::{
        self, AnyElement, Axis, Context, DragMoveEvent, Empty, Entity, Focusable, Render,
        Subscription, Window, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        menu::{self, Cursor, Hit, Item},
        popover, tabs,
        titlebar::{self, CaptionSide},
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use std::{collections::HashMap, path::PathBuf};

gpui::actions!(
    session_panel,
    [
        OpenFile,
        NewTerminal,
        CloseTab,
        ToggleFiles,
        NextTab,
        PrevTab
    ]
);

struct FilesResize;

enum Content {
    Review(Entity<Changes>),
    #[cfg(feature = "desktop")]
    Terminal(Entity<Terminal>),
    File(Entity<FileView>),
}
struct Tab {
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
    /// The row: which tabs are open, in what order, and which is in front.
    strip: tabs::Strip<usize>,
    /// What each of the strip's ids holds. Keyed rather than held in the strip
    /// so the order is arithmetic the strip can do without a window.
    contents: HashMap<usize, Tab>,
    next_id: usize,
    menu: bool,
    cursor: Cursor,
    closing: Option<usize>,
    focus_pending: bool,
    restore_pending: Option<persistence::SavedPanel>,
    /// The press on the strip's [`chrome::grip`].
    drag: titlebar::DragState,
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
            strip: tabs::Strip::new(),
            contents: HashMap::new(),
            next_id: 0,
            menu: false,
            cursor: Cursor::default(),
            closing: None,
            focus_pending: false,
            restore_pending: None,
            drag: Default::default(),
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
        self.contents.insert(
            id,
            Tab {
                content,
                _subscriptions: subscriptions,
            },
        );
        self.strip.open(id);
        cx.notify();
        id
    }

    /// The tabs in the order the strip holds them.
    fn ordered(&self) -> impl Iterator<Item = (usize, &Tab)> {
        self.strip
            .tabs()
            .iter()
            .filter_map(|id| self.contents.get(id).map(|tab| (*id, tab)))
    }

    /// What the tab in front holds.
    fn front(&self) -> Option<&Tab> {
        self.strip.active().and_then(|id| self.contents.get(id))
    }

    pub fn review(&mut self, cx: &mut Context<Self>) {
        let open = self
            .ordered()
            .find(|(_, tab)| matches!(tab.content, Content::Review(_)))
            .map(|(id, _)| id);
        if let Some(id) = open {
            self.strip.activate(&id);
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

    /// Nothing without the `desktop` feature: there is no shell to run.
    #[cfg(not(feature = "desktop"))]
    fn terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = (window, cx);
    }

    #[cfg(feature = "desktop")]
    fn terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = cx.new(|cx| Terminal::new(&self.cwd, cx));
        let id = self.next_id;
        let exit = cx.subscribe_in(&terminal, window, move |this, _, _: &Exited, window, cx| {
            let focused = this.contents.get(&id).is_some_and(|tab| {
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
        if let Some(tab) = self.front() {
            match &tab.content {
                #[cfg(feature = "desktop")]
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

    fn toggle_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.files_open {
            self.files_open = false;
            self.focus(window, cx);
            cx.notify();
        } else {
            self.files(window, cx);
        }
    }

    fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.focus_pending = true;
        let path = path.canonicalize().unwrap_or(path);
        let open = self
            .ordered()
            .find(|(_, tab)| matches!(&tab.content, Content::File(file) if file.read(cx).path == path))
            .map(|(id, _)| id);
        if let Some(id) = open {
            self.strip.activate(&id);
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

    /// Step to the tab `step` along, wrapping at the ends — the row is a ring,
    /// the way a browser's is.
    ///
    /// The focus goes with it: the chord is pressed with the hand in the panel,
    /// and a tab shown without the focus following leaves the next keystroke in
    /// the one that was left.
    fn cycle(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.strip.len() < 2 {
            return;
        }
        self.strip.cycle(step);
        self.focus(window, cx);
        cx.notify();
    }

    fn remove(&mut self, id: usize, cx: &mut Context<Self>) {
        if !self.strip.close(&id) {
            return;
        }
        self.contents.remove(&id);
        if self.closing == Some(id) {
            self.closing = None;
        }
        cx.notify();
    }

    fn close(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let dirty = matches!(
            self.contents.get(&id).map(|tab| &tab.content),
            Some(Content::File(file)) if file.read(cx).dirty(cx)
        );
        if dirty {
            self.closing = Some(id);
            self.strip.activate(&id);
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
            Item::action("Terminal")
                .with_icon(icons::development::Terminal)
                .with_shortcut_in(&NewTerminal, "SessionPanel", window),
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
        let right = chrome::has(CaptionSide::Right, window, cx);
        let items = Self::items(window);
        let rows = items.clone();
        let popup = self.menu.then(|| {
            menu::card(
                &theme,
                "panel-menu",
                &items,
                &self.cursor,
                window,
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
        let active_file = self.front().and_then(|tab| {
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
            .front()
            .map(|tab| match &tab.content {
                Content::File(file) => {
                    file.update(cx, |file, cx| file.status_bar(self.files_open, window, cx))
                }
                Content::Review(review) => {
                    review.update(cx, |review, cx| review.status_bar(self.files_open, cx))
                }
                #[cfg(feature = "desktop")]
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
            .front()
            .map(|tab| match &tab.content {
                Content::Review(review) => review.clone().into_any_element(),
                #[cfg(feature = "desktop")]
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
                this.toggle_files(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenFile, window, cx| this.files(window, cx)))
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.terminal(window, cx)))
            .on_action(
                cx.listener(|this, _: &crate::view::menubar::CloseWindow, window, cx| {
                    if let Some(id) = this.strip.active().copied() {
                        this.close(id, window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                if let Some(id) = this.strip.active().copied() {
                    this.close(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.cycle(1, window, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| this.cycle(-1, window, cx)))
            .child(
                crate::view::root::band()
                    .gap(px(6.))
                    // Always at the window's top right while it is up.
                    .when(right, |band| band.pr_0())
                    .child(
                        tabs::bar("panel-tabs").children(self.ordered().map(|(id, tab)| {
                            let icon = match &tab.content {
                                Content::Review(_) => icons::development::GitCompare,
                                #[cfg(feature = "desktop")]
                                Content::Terminal(_) => icons::development::Terminal,
                                Content::File(_) => icons::files::File,
                            };
                            // The path is the tooltip and the last component is
                            // the name: several tabs can be named the same.
                            let (name, path, dirty) = match &tab.content {
                                Content::Review(_) => {
                                    ("Review".to_string(), "Review".to_string(), false)
                                }
                                #[cfg(feature = "desktop")]
                                Content::Terminal(terminal) => {
                                    let path = &terminal.read(cx).directory;
                                    (
                                        path.file_name()
                                            .unwrap_or(path.as_os_str())
                                            .to_string_lossy()
                                            .into_owned(),
                                        path.display().to_string(),
                                        false,
                                    )
                                }
                                Content::File(file) => {
                                    let file = file.read(cx);
                                    (
                                        file.path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned(),
                                        file.path.display().to_string(),
                                        file.dirty(cx),
                                    )
                                }
                            };
                            let mut label = tabs::Label::new(name).with_icon(icon);
                            // Unsaved work is the mark rather than a bullet in
                            // the name: the name truncates and the mark does
                            // not.
                            if dirty {
                                label = label
                                    .mark(icons::Icon::glyph(icons::glyph::CircleSmall).solid());
                            }
                            let state = match self.strip.active() == Some(&id) {
                                true => tabs::State::Focused,
                                false => tabs::State::Resting,
                            };
                            let key = gpui::SharedString::from(format!("panel-{id}"));
                            tabs::tab(&theme, key.clone(), label, state)
                                .tooltip(move |window, cx| Tooltip::text(path.clone(), window, cx))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.strip.activate(&id);
                                    this.focus(window, cx);
                                    cx.notify();
                                }))
                                .child(tabs::close(&theme, key, tabs::Close::OnHover).on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close(id, window, cx);
                                    }),
                                ))
                        })),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .child(
                                theme
                                    .icon_button(
                                        icons::math::Plus,
                                        ButtonStyle::Ghost,
                                        Some(Fade::new(Painter::of(cx), "panel-add")),
                                    )
                                    .id("panel-add")
                                    .flex_none()
                                    .tooltip(|window, cx| Tooltip::text("New tab", window, cx))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.menu = !this.menu;
                                        this.cursor.clear();
                                        cx.notify();
                                    })),
                            )
                            .children(popup.map(|popup| {
                                popover::anchored_menu_below(
                                    "panel-menu",
                                    popup.into_any_element(),
                                    None,
                                )
                            })),
                    )
                    .child(chrome::grip("panel-grip", &self.drag, window))
                    .child(
                        // The column it acts on, which is this one: a
                        // left-panel glyph on the right panel's own hide
                        // button pointed at the wrong side of the window.
                        theme
                            .icon_button(
                                icons::layout::PanelRight,
                                ButtonStyle::Ghost,
                                Some(Fade::new(Painter::of(cx), "panel-hide")),
                            )
                            .id("panel-hide")
                            .flex_none()
                            .tooltip(|window, cx| Tooltip::text("Hide right panel", window, cx))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleChanges), cx)
                            }),
                    )
                    .children(chrome::caption(CaptionSide::Right, window, cx)),
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
                                            let file = this.contents.get(&id).and_then(|tab| {
                                                match &tab.content {
                                                    Content::File(file) => Some(file.clone()),
                                                    _ => None,
                                                }
                                            });
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
                    .when(!(self.files_open && self.strip.is_empty()), |row| {
                        row.child(div().flex_1().min_w_0().child(body))
                    })
                    .when(self.files_open, |row| {
                        row.children(self.files.clone().map(|files| {
                            div()
                                .when(self.strip.is_empty(), |tree| tree.flex_1().w_full())
                                .when(!self.strip.is_empty(), |tree| {
                                    tree.w(px(self.files_width))
                                        .max_w(gpui::relative(0.8))
                                        .flex_none()
                                })
                                .relative()
                                .child(files)
                                .when(!self.strip.is_empty(), |tree| {
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
    pub(crate) fn open_session_file(
        &mut self,
        link: &super::transcript::links::OpenSessionFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.showing(cx) != Some(Pane::Chat)
            || self.workspace.read(cx).active_id() != Some(link.session)
        {
            return;
        }
        self.set_changes_open(true, cx);
        self.sync_changes(cx);
        if let Some(panel) = self.changes.clone() {
            panel.update(cx, |panel, cx| {
                panel.restore_tabs(window, cx);
                panel.open_file(link.path.clone(), cx);
                if let Some(line) = link.line
                    && let Some(tab) = panel.front()
                    && let Content::File(file) = &tab.content
                {
                    file.update(cx, |file, cx| file.go_to_line(line, cx));
                }
                panel.focus(window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn toggle_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.changes_open
            && let Some(panel) = self.changes.clone()
        {
            panel.update(cx, |panel, cx| panel.toggle_files(window, cx));
        } else {
            self.show_files(window, cx);
        }
    }

    pub(crate) fn show_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_changes_open(true, cx);
        self.sync_changes(cx);
        if let Some(panel) = self.changes.clone() {
            panel.update(cx, |panel, cx| panel.files(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn show_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_changes_open(true, cx);
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

    pub(crate) fn toggle_changes(
        &mut self,
        _: &ToggleChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_changes_open(!self.changes_open, cx);
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

    /// Put the panel up or down for the directory in front, and write that
    /// down.
    ///
    /// Written on the change and not at the quit: ⌘Q, a crash and a killed
    /// `cargo run` all end the process without running a release hook, which is
    /// the same reason the panel's width is written on a settle — see
    /// [`Cydonia::save_panel_layout_settled`].
    fn set_changes_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.changes_open = open;
        // Which directory this is about, in case the first sync of the frame
        // has not run yet: leaving it unanswered would let `follow_changes`
        // read the saved bit back over what was just pressed.
        self.changes_for = self.changes_for.take().or_else(|| self.shell_cwd(cx));
        if let Some(cwd) = self.changes_for.clone() {
            self.changes_shown.insert(cwd, open);
        }
        self.save_panel_layout_settled(cx);
    }

    /// Carry [`Cydonia::changes_open`] from one directory to the next.
    ///
    /// The panel is a property of the directory in front, not of the window:
    /// what it was left at there is what it comes back as, and a directory
    /// nobody has opened it in gets no panel. A directory first seen this run
    /// is read off disk once and kept, which is what survives a quit.
    fn follow_changes(&mut self, cx: &mut Context<Self>) {
        let active = self.shell_cwd(cx);
        if self.changes_for == active {
            return;
        }
        if let Some(previous) = self.changes_for.take() {
            self.changes_shown.insert(previous, self.changes_open);
        }
        self.changes_open = match &active {
            Some(cwd) => match self.changes_shown.get(cwd) {
                Some(open) => *open,
                None => {
                    let open = persistence::saved_panel(cwd).is_some_and(|saved| saved.open);
                    self.changes_shown.insert(cwd.clone(), open);
                    open
                }
            },
            None => false,
        };
        self.changes_for = active;
    }

    pub(crate) fn sync_changes(&mut self, cx: &mut Context<Self>) {
        self.follow_changes(cx);
        let here = self.shell_cwd(cx);
        // A panel outlives the directory going out of front, and is dropped
        // once nothing open leads back to it — except while it holds work
        // nobody has saved, which no amount of closing may throw away.
        let reachable = self.panel_directories(cx);
        self.right_panels.retain(|cwd, panel| {
            reachable.contains(cwd)
                || panel.read(cx).ordered().any(
                    |(_, tab)| matches!(&tab.content, Content::File(file) if file.read(cx).dirty(cx)),
                )
        });
        self.changes = here.filter(|_| self.changes_open).map(|cwd| {
            self.right_panels
                .entry(cwd.clone())
                .or_insert_with(|| {
                    cx.new(|cx| {
                        let mut panel = Panel::new(cwd.clone(), cx);
                        panel.restore_pending = persistence::saved_panel(&cwd);
                        panel.project_root = cwd.canonicalize().unwrap_or(cwd);
                        panel
                    })
                })
                .clone()
        });
    }

    /// Every directory the window can still reach a panel through: the open
    /// projects, and the working directory of each of their sessions.
    ///
    /// A session's own directory is in here because it is not always its
    /// project's — a session on a worktree has its diff, its tree and its
    /// shells in the worktree.
    fn panel_directories(&self, cx: &gpui::App) -> std::collections::HashSet<std::path::PathBuf> {
        let workspace = self.workspace.read(cx);
        workspace
            .projects
            .iter()
            .flat_map(|project| {
                std::iter::once(project.path.clone())
                    .chain(project.sessions.iter().map(|chat| chat.cwd.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/panel.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/right_panel.rs"]
mod right_panel_tests;
