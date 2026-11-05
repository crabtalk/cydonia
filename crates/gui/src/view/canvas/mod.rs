use crate::{
    model::{
        canvas::Canvas,
        workspace::{Entry, Member, Opening},
    },
    view::{
        kit::menu::{self, MenuHost},
        window::{
            reach::InWindow,
            root::{Cydonia, NewCanvas},
        },
    },
};
use artifact::kind::Kind;
use bezel::{
    gpui::{
        App, AppContext as _, Context, Entity, FocusHandle, Focusable, Subscription, WeakEntity,
        Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        menu::Item,
        tooltip::Tooltip,
        widgets::{Button, Scaffolding as _},
    },
};
use canvas::{
    Arrange, CanvasEditor, CanvasEvent, CanvasView as Surface, Kinds, Snap, handle,
    kind::Sizing,
    layout,
    model::{self, End, Node},
};
use std::path::PathBuf;

mod embed;
mod sources;
mod transfer;

const CARD_WIDTH: i64 = 280;
const CARD_HEIGHT: i64 = 160;
const GROUP_WIDTH: i64 = 400;
const GROUP_HEIGHT: i64 = 280;
const GROUP_PADDING: i64 = 24;
const COLORS: [(&str, &str); 6] = [
    ("1", "Red"),
    ("2", "Orange"),
    ("3", "Yellow"),
    ("4", "Green"),
    ("5", "Cyan"),
    ("6", "Purple"),
];

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Menu {
    Actions,
    Context,
}

pub(crate) struct CanvasView {
    pub(crate) canvas: Entry,
    source: Entity<Canvas>,
    root: WeakEntity<Cydonia>,
    path: PathBuf,
    pub(crate) surface: Entity<Surface>,
    menu: ContextMenu<Self, Menu>,
    at: (i64, i64),
    _events: Option<Subscription>,
}

impl Cydonia {
    pub(crate) fn new_canvas_action(
        &mut self,
        _: &NewCanvas,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.workspace.read(cx).active {
            self.new_canvas(project, window, cx);
        }
    }

    pub(crate) fn new_canvas(
        &mut self,
        project: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_project(project, cx);
        let made = self
            .workspace
            .update(cx, |workspace, cx| workspace.create_canvas(project, cx));
        match made {
            Ok(member) => self.enter_member(member, Opening::Kept, window, cx),
            Err(message) => self.show_error("Cannot create canvas".into(), message, window, cx),
        }
    }

    pub(crate) fn open_canvas(
        &mut self,
        project: usize,
        ix: usize,
        opening: Opening,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit(cx);
        let Some(open) = self.workspace.read(cx).projects.get(project) else {
            return;
        };
        let Some(canvas) = open.canvases.get(ix) else {
            return;
        };
        let member = Member::new(open.id, Kind::Canvas, canvas.id.clone());
        self.enter_member(member, opening, window, cx);
    }
}

impl CanvasView {
    pub(crate) fn new(
        canvas: Entry,
        source: Entity<Canvas>,
        root: WeakEntity<Cydonia>,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        let document = source.read(cx).document.clone();
        let mut kinds = Kinds::new().with_root(path.clone()).with_fresh(|| Node {
            kind: model::TEXT.into(),
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
            text: Some(String::new()),
            ..Node::default()
        });
        for name in [model::TEXT, model::FILE, model::LINK, model::GROUP] {
            let mut declared = kinds.get(name).clone().handles(handle::frame);
            if name == model::TEXT {
                declared = declared.sizing(Sizing::Fixed);
            }
            kinds = kinds.with(name, declared);
        }
        kinds = sources::kinds(kinds, path.clone());
        let surface = cx.new(|cx| {
            Surface::for_document(document, layout::FREE, cx)
                .with_kinds(kinds)
                .with_options(canvas::Options {
                    external_drops: true,
                    ..Default::default()
                })
                .with_snap(Snap {
                    grid: Some(20),
                    guides: true,
                })
        });
        cx.observe(&source, |_, _, cx| cx.notify()).detach();
        cx.observe(&surface, |_, _, cx| cx.notify()).detach();
        Self {
            canvas,
            source,
            root,
            path,
            surface,
            menu: ContextMenu::default(),
            at: (0, 0),
            _events: None,
        }
    }

    fn add_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.at;
        self.surface.update(cx, |surface, cx| {
            surface.update_editor(cx, |editor| editor.add_root(at));
            surface.edit_selected(window, cx);
        });
    }

    fn add_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at = self.at;
        self.surface.update(cx, |surface, cx| {
            surface.update_editor(cx, |editor| {
                if editor.selected_nodes().is_empty() {
                    editor.add_node(
                        Node {
                            kind: model::GROUP.into(),
                            width: GROUP_WIDTH,
                            height: GROUP_HEIGHT,
                            label: Some("Group".into()),
                            ..Node::default()
                        },
                        (at.0 - GROUP_WIDTH / 2, at.1 - GROUP_HEIGHT / 2),
                    );
                } else {
                    editor.group_selected("Group", GROUP_PADDING);
                }
            });
            surface.edit_selected(window, cx);
        });
    }

    fn rows(&self, cx: &App) -> Vec<(Item, menu::Act<Self>)> {
        let editor = self.surface.read(cx).editor();
        let read_only = editor.options().read_only;
        let selected = !editor.selection().is_empty();
        let nodes = editor.selected_nodes().len();
        let mut rows = vec![
            menu::row(
                Item::action("Add file cards…").disabled(read_only),
                |this: &mut Self, _, cx| this.pick_files(None, cx),
            ),
            menu::row(
                Item::action("Add entry card…").disabled(read_only),
                |this: &mut Self, window, cx| this.pick_reference(None, window, cx),
            ),
            menu::row(
                Item::action("Import canvas…"),
                |this: &mut Self, window, cx| {
                    let project = this.canvas.project;
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.import_canvas(project, window, cx)
                    });
                },
            ),
            menu::row(
                Item::action("Export JSON Canvas…"),
                |this: &mut Self, window, cx| this.export(window, cx),
            ),
            menu::row(
                Item::action("Add text card")
                    .with_icon(icons::text::Type)
                    .disabled(read_only),
                |this: &mut Self, window, cx| this.add_text(window, cx),
            ),
            menu::row(
                Item::action(if nodes > 0 {
                    "Group selection"
                } else {
                    "Add group"
                })
                .with_icon(icons::layout::PanelsTopLeft)
                .disabled(read_only),
                |this: &mut Self, window, cx| this.add_group(window, cx),
            ),
            menu::row(
                Item::action("Edit").disabled(read_only || !selected),
                |this: &mut Self, window, cx| {
                    this.surface
                        .update(cx, |surface, cx| surface.edit_selected(window, cx))
                },
            ),
            menu::row(
                Item::action("Undo").disabled(!editor.can_undo()),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            editor.undo();
                        })
                    });
                },
            ),
            menu::row(
                Item::action("Redo").disabled(!editor.can_redo()),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            editor.redo();
                        })
                    });
                },
            ),
        ];
        if selected && !read_only {
            let mut colors = vec![menu::row(
                Item::action("Default"),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| editor.set_selection_color(None))
                    })
                },
            )];
            colors.extend(COLORS.into_iter().map(|(value, label)| {
                menu::row(Item::action(label), move |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            editor.set_selection_color(Some(value.into()))
                        })
                    })
                })
            }));
            rows.push(menu::submenu("Color", icons::glyph::Palette, colors));
            if editor.selected_edge().is_some() {
                let directions = [
                    ("Nondirectional", End::None, End::None),
                    ("Unidirectional", End::None, End::Arrow),
                    ("Bidirectional", End::Arrow, End::Arrow),
                ]
                .into_iter()
                .map(|(label, from, to)| {
                    menu::row(Item::action(label), move |this: &mut Self, _, cx| {
                        this.surface.update(cx, |surface, cx| {
                            surface.update_editor(cx, |editor| editor.set_connection_ends(from, to))
                        })
                    })
                })
                .collect();
                rows.push(menu::submenu(
                    "Line direction",
                    icons::arrows::ArrowLeftRight,
                    directions,
                ));
            }
            if nodes >= 2 {
                let operations = [
                    ("Align left", Arrange::Left),
                    ("Align center", Arrange::Center),
                    ("Align right", Arrange::Right),
                    ("Align top", Arrange::Top),
                    ("Align middle", Arrange::Middle),
                    ("Align bottom", Arrange::Bottom),
                    ("Distribute horizontally", Arrange::DistributeHorizontal),
                    ("Distribute vertically", Arrange::DistributeVertical),
                    ("Arrange in a row", Arrange::Row { gap: GROUP_PADDING }),
                    (
                        "Arrange in a column",
                        Arrange::Column { gap: GROUP_PADDING },
                    ),
                    (
                        "Arrange in a grid",
                        Arrange::Grid {
                            columns: (nodes as f64).sqrt().ceil() as usize,
                            gap: GROUP_PADDING,
                        },
                    ),
                    ("Justify horizontally", Arrange::JustifyHorizontal),
                    ("Justify vertically", Arrange::JustifyVertical),
                ];
                rows.push(menu::submenu(
                    "Align and arrange",
                    icons::glyph::TextAlignStart,
                    operations
                        .into_iter()
                        .map(|(label, command)| {
                            menu::row(Item::action(label), move |this: &mut Self, _, cx| {
                                this.surface.update(cx, |surface, cx| {
                                    surface.update_editor(cx, |editor| editor.arrange(command))
                                })
                            })
                        })
                        .collect(),
                ));
            }
            rows.push(menu::row(
                Item::action("Duplicate"),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, CanvasEditor::duplicate)
                    })
                },
            ));
            rows.push(menu::row(
                Item::action("Remove").with_icon(icons::files::Trash),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, CanvasEditor::remove_selected)
                    })
                },
            ));
        }
        if let Some(node) = editor
            .selected_nodes()
            .first()
            .and_then(|id| editor.canvas().node(id))
        {
            if matches!(node.kind.as_str(), model::FILE | model::LINK) {
                let file_id = node.id.clone();
                rows.push(menu::row(
                    Item::action("Replace with file…").disabled(read_only),
                    move |this: &mut Self, _, cx| this.pick_files(Some(file_id.clone()), cx),
                ));
                let id = node.id.clone();
                rows.push(menu::row(
                    Item::action("Replace reference…").disabled(read_only),
                    move |this: &mut Self, window, cx| {
                        this.pick_reference(Some(id.clone()), window, cx)
                    },
                ));
                let node = (*node).clone();
                rows.push(menu::row(
                    Item::action("Open source"),
                    move |this: &mut Self, window, cx| this.open_source(&node, window, cx),
                ));
            } else if node.kind == model::TEXT && !read_only {
                let id = node.id.clone();
                rows.push(menu::row(
                    Item::action("Convert to article"),
                    move |this: &mut Self, window, cx| this.convert(&id, window, cx),
                ));
            }
        }
        let snap = editor.snap();
        rows.extend([
            menu::row(
                Item::action("Snap to grid").checked(snap.grid.is_some()),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            let mut snap = editor.snap();
                            snap.grid = if snap.grid.is_some() { None } else { Some(20) };
                            editor.set_snap(snap);
                        })
                    })
                },
            ),
            menu::row(
                Item::action("Snap to objects").checked(snap.guides),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            let mut snap = editor.snap();
                            snap.guides = !snap.guides;
                            editor.set_snap(snap);
                        })
                    })
                },
            ),
            menu::row(
                Item::action("Read-only").checked(read_only),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, |editor| {
                            let mut options = editor.options();
                            options.read_only = !options.read_only;
                            editor.set_options(options);
                        })
                    })
                },
            ),
        ]);
        rows.into_iter()
            .map(|(item, act)| {
                let action: menu::Act<Self> = Box::new(move |this, path, window, cx| {
                    window.focus(&this.surface.read(cx).focus_handle(cx), cx);
                    act(this, path, window, cx);
                });
                (item, action)
            })
            .collect()
    }

    fn navigate(
        &mut self,
        command: fn(&mut CanvasEditor),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.surface
            .update(cx, |surface, cx| surface.update_editor(cx, command));
        window.focus(&self.surface.read(cx).focus_handle(cx), cx);
    }
}

impl InWindow for CanvasView {
    fn root(&self) -> &WeakEntity<Cydonia> {
        &self.root
    }
}

impl MenuHost for CanvasView {
    type Key = Menu;
    fn menu(&self) -> &ContextMenu<Self, Menu> {
        &self.menu
    }
    fn menu_mut(&mut self) -> &mut ContextMenu<Self, Menu> {
        &mut self.menu
    }
}
impl Focusable for CanvasView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.surface.read(cx).focus_handle(cx)
    }
}
impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self._events.is_none() {
            self._events = Some(cx.subscribe_in(
                &self.surface,
                window,
                |this, _, event: &CanvasEvent, window, cx| {
                    if let CanvasEvent::FilesDropped { at, paths } = event {
                        this.at = (at.x.round() as i64, at.y.round() as i64);
                        this.add_files(paths.clone(), None, cx);
                    }
                    if let CanvasEvent::ContextMenu { position, at, .. } = event {
                        this.at = (at.x.round() as i64, at.y.round() as i64);
                        this.toggle_menu(
                            Menu::Context,
                            Place::At(*position),
                            |this, _, cx| this.rows(cx),
                            window,
                            cx,
                        );
                    }
                },
            ));
        }
        let theme = Theme::of(cx).clone();
        let read_only = self.surface.read(cx).editor().options().read_only;
        let error = self.source.read(cx).error.clone();
        let actions = menu::button(
            "canvas-actions",
            icons::account::Settings,
            menu::Reveal::Always,
            self.menu.key() == Some(&Menu::Actions),
            cx,
        )
        .tooltip(|window, cx| Tooltip::text("Canvas actions", window, cx))
        .on_click(cx.listener(|this, _, window, cx| {
            this.at = this.surface.read(cx).editor().center();
            this.toggle_menu(
                Menu::Actions,
                Place::Above,
                |this, _, cx| this.rows(cx),
                window,
                cx,
            );
        }));
        let actions = self
            .menu_trigger(actions, Menu::Actions, cx)
            .children(self.menu_here(&Menu::Actions, "canvas-actions-menu", window, cx));
        let tools = theme
            .group_box()
            .flex_row()
            .p(px(4.))
            .gap(px(2.))
            .child(
                Button::new("canvas-add-text", "Text")
                    .icon(icons::text::Type)
                    .enabled(!read_only)
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.at = this.surface.read(cx).editor().center();
                        this.add_text(window, cx);
                    })),
            )
            .child(
                Button::new("canvas-add-group", "Group")
                    .icon(icons::layout::PanelsTopLeft)
                    .enabled(!read_only)
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.at = this.surface.read(cx).editor().center();
                        this.add_group(window, cx);
                    })),
            )
            .child(actions);
        let zoom = theme
            .group_box()
            .flex_row()
            .p(px(4.))
            .gap(px(2.))
            .child(
                Button::new("canvas-zoom-out", "Zoom out")
                    .icon(icons::glyph::Minus)
                    .label_hidden()
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.navigate(CanvasEditor::zoom_out, window, cx)
                    })),
            )
            .child(
                Button::new("canvas-fit", "Zoom to fit")
                    .icon(icons::layout::Maximize)
                    .label_hidden()
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.navigate(CanvasEditor::fit, window, cx)
                    })),
            )
            .child(
                Button::new("canvas-zoom-in", "Zoom in")
                    .icon(icons::math::Plus)
                    .label_hidden()
                    .on_press(cx.listener(|this, _, window, cx| {
                        this.navigate(CanvasEditor::zoom_in, window, cx)
                    })),
            );
        div()
            .on_drop(cx.listener(
                |this,
                 carried: &bezel::ui::drag::Carry<crate::view::sidebar::Dragged>,
                 window,
                 cx| {
                    let at = this.surface.read(cx).canvas_point(window.mouse_position());
                    this.at = (at.x.round() as i64, at.y.round() as i64);
                    if this.drop_entry(carried.item(), cx) {
                        cx.stop_propagation();
                    }
                },
            ))
            .flex_1()
            .min_w_0()
            .size_full()
            .min_h_0()
            .relative()
            .bg(theme.surface)
            .child(self.surface.clone())
            .child(
                div()
                    .absolute()
                    .bottom(px(12.))
                    .left(px(12.))
                    .right(px(12.))
                    .flex()
                    .justify_between()
                    .gap(px(8.))
                    .child(tools)
                    .child(zoom),
            )
            .when_some(error, |element, message| {
                element.child(
                    theme
                        .group_box()
                        .absolute()
                        .top(px(12.))
                        .left(px(12.))
                        .right(px(12.))
                        .p(px(12.))
                        .gap(px(8.))
                        .child(
                            div()
                                .text_style(TextStyle::Callout)
                                .text_color(theme.danger)
                                .child(message),
                        )
                        .child(
                            Button::new("canvas-reload", "Reload saved canvas").on_press(
                                cx.listener(|this, _, window, cx| {
                                    this.source.update(cx, |source, cx| source.reload(cx));
                                    window.focus(&this.focus_handle(cx), cx);
                                }),
                            ),
                        )
                        .child(Button::new("canvas-keep", "Save my version").on_press(
                            cx.listener(|this, _, window, cx| {
                                this.source.update(cx, |source, cx| source.keep(cx));
                                window.focus(&this.focus_handle(cx), cx);
                            }),
                        )),
                )
            })
            .children(self.menu_here(&Menu::Context, "canvas-context-menu", window, cx))
    }
}
