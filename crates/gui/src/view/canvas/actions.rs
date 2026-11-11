use crate::view::{
    canvas::{CanvasView, GROUP_PADDING},
    kit::menu,
    window::reach::InWindow,
};
use bezel::{
    gpui::{App, Focusable},
    ui::{icons, menu::Item},
};
use canvas::{
    Arrange, CanvasEditor,
    model::{self, End, TextMode},
};

const TEXT_SIZES: [(f32, &str); 7] = [
    (12., "12"),
    (16., "16"),
    (24., "24"),
    (32., "32"),
    (48., "48"),
    (64., "64"),
    (96., "96"),
];
const COLORS: [(&str, &str); 6] = [
    ("1", "Red"),
    ("2", "Orange"),
    ("3", "Yellow"),
    ("4", "Green"),
    ("5", "Cyan"),
    ("6", "Purple"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Background,
    Selection,
    Actions,
}

impl CanvasView {
    pub(super) fn rows(&self, scope: Scope, cx: &App) -> Vec<(Item, menu::Act<Self>)> {
        let editor = self.surface.read(cx).editor();
        let read_only = editor.options().read_only;
        let canvas_scope = matches!(scope, Scope::Background | Scope::Actions);
        let selected = scope != Scope::Background && !editor.selection().is_empty();
        let nodes = if selected {
            editor.selected_nodes().len()
        } else {
            0
        };
        let single = selected && editor.selection().len() == 1;
        let navigation = if nodes > 0 {
            vec![
                menu::row(
                    Item::action("Fit to content")
                        .disabled(!self.surface.read(cx).can_fit_content()),
                    |this: &mut Self, _, cx| {
                        this.surface
                            .update(cx, |surface, cx| surface.fit_to_content(cx))
                    },
                ),
                menu::row(
                    Item::action("Zoom to selection"),
                    |this: &mut Self, window, cx| {
                        this.navigate(CanvasEditor::zoom_to_selection, window, cx)
                    },
                ),
            ]
        } else {
            Vec::new()
        };
        let creation = vec![
            menu::row(
                Item::action("Link card").disabled(read_only),
                |this: &mut Self, window, cx| this.add_link(window, cx),
            ),
            menu::row(
                Item::action("File cards…").disabled(read_only),
                |this: &mut Self, _, cx| this.pick_files(None, cx),
            ),
            menu::row(
                Item::action("Entry card…").disabled(read_only),
                |this: &mut Self, window, cx| this.pick_reference(None, window, cx),
            ),
            menu::row(
                Item::action("Text")
                    .with_icon(icons::text::Type)
                    .disabled(read_only),
                |this: &mut Self, window, cx| this.add_text(window, cx),
            ),
            menu::row(
                Item::action("Text card")
                    .with_icon(icons::text::Type)
                    .disabled(read_only),
                |this: &mut Self, window, cx| this.add_text_card(window, cx),
            ),
            menu::row(
                Item::action("Group")
                    .with_icon(icons::layout::PanelsTopLeft)
                    .disabled(read_only),
                |this: &mut Self, window, cx| this.add_group(window, cx),
            ),
        ];
        let mut selection = if single {
            vec![menu::row(
                Item::action("Edit").disabled(read_only),
                |this: &mut Self, window, cx| {
                    this.surface
                        .update(cx, |surface, cx| surface.edit_selected(window, cx))
                },
            )]
        } else {
            Vec::new()
        };
        if nodes > 0 {
            selection.push(menu::row(
                Item::action("Group selection")
                    .with_icon(icons::layout::PanelsTopLeft)
                    .disabled(read_only),
                |this: &mut Self, window, cx| this.group_selection(window, cx),
            ));
        }
        let history = vec![
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
        let mut transfer = vec![menu::row(
            Item::action("Import canvas…"),
            |this: &mut Self, window, cx| {
                let project = this.canvas.project;
                this.defer_window(window, cx, move |root, window, cx| {
                    root.import_canvas(project, window, cx)
                });
            },
        )];
        let mut exports = vec![menu::row(
            Item::action("JSON Canvas…"),
            |this: &mut Self, window, cx| this.export(window, cx),
        )];
        if selected && !read_only {
            let has_text = editor.selected_nodes().iter().any(|id| {
                editor
                    .canvas()
                    .node(id)
                    .is_some_and(|node| node.kind == model::TEXT)
            });
            if has_text || editor.selected_edge().is_some() {
                selection.push(menu::switch(
                    "Text size",
                    icons::text::Type,
                    TEXT_SIZES
                        .into_iter()
                        .map(|(size, label)| (Some(size), icons::text::Type.into(), label))
                        .collect(),
                    editor.selection_text_size(),
                    true,
                    |this: &mut Self, size, _, cx| {
                        this.surface.update(cx, |surface, cx| {
                            surface.update_editor(cx, |editor| editor.set_selection_text_size(size))
                        });
                    },
                ));
            }
            if has_text {
                selection.push(menu::switch(
                    "Text presentation",
                    icons::text::Type,
                    [
                        (
                            Some(TextMode::Label),
                            icons::text::Type.into(),
                            "Plain text",
                        ),
                        (
                            Some(TextMode::Card),
                            icons::text::Type.into(),
                            "Document card",
                        ),
                    ]
                    .into(),
                    editor.selection_text_mode(),
                    false,
                    |this: &mut Self, mode, _, cx| {
                        if let Some(mode) = mode {
                            this.surface.update(cx, |surface, cx| {
                                surface.update_editor(cx, |editor| {
                                    editor.set_selection_text_mode(mode)
                                })
                            });
                        }
                    },
                ));
            }
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
            selection.push(menu::submenu("Color", icons::glyph::Palette, colors));
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
                selection.push(menu::submenu(
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
                selection.push(menu::submenu(
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
            selection.push(menu::row(
                Item::action("Duplicate"),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, CanvasEditor::duplicate)
                    })
                },
            ));
            selection.push(menu::row(
                Item::action("Remove").with_icon(icons::files::Trash),
                |this: &mut Self, _, cx| {
                    this.surface.update(cx, |surface, cx| {
                        surface.update_editor(cx, CanvasEditor::remove_selected)
                    })
                },
            ));
        }
        if single
            && let Some(node) = editor
                .selected_nodes()
                .first()
                .and_then(|id| editor.canvas().node(id))
        {
            if matches!(node.kind.as_str(), model::FILE | model::LINK) {
                let file_id = node.id.clone();
                let file = menu::row(
                    Item::action("File…").disabled(read_only),
                    move |this: &mut Self, _, cx| this.pick_files(Some(file_id.clone()), cx),
                );
                let id = node.id.clone();
                let reference = menu::row(
                    Item::action("Reference…").disabled(read_only),
                    move |this: &mut Self, window, cx| {
                        this.pick_reference(Some(id.clone()), window, cx)
                    },
                );
                selection.push(menu::submenu(
                    "Replace",
                    icons::files::File,
                    vec![file, reference],
                ));
                let id = node.id.clone();
                selection.push(menu::row(
                    Item::action(if node.kind == model::LINK {
                        "Open link"
                    } else {
                        "Open source"
                    }),
                    move |this: &mut Self, window, cx| {
                        this.surface.update(cx, |surface, cx| {
                            surface.open_node(&id, window, cx);
                        });
                    },
                ));
            } else if node.kind == model::TEXT && !read_only {
                let id = node.id.clone();
                selection.push(menu::row(
                    Item::action("Convert to article"),
                    move |this: &mut Self, window, cx| this.convert(&id, window, cx),
                ));
            }
        }
        exports.extend(
            [
                ("Full canvas image", canvas::export::Area::Canvas),
                ("Current viewport image", canvas::export::Area::Viewport),
            ]
            .into_iter()
            .map(|(label, area)| {
                let frame = area.frame(editor).ok();
                let scales = [1., 2., 4.]
                    .into_iter()
                    .map(|scale| {
                        let label = frame
                            .as_ref()
                            .map(|frame| {
                                format!(
                                    "{scale}× · {} × {}",
                                    (frame.size.width * scale).ceil() as u32,
                                    (frame.size.height * scale).ceil() as u32
                                )
                            })
                            .unwrap_or_else(|| format!("{scale}×"));
                        menu::row(
                            Item::action(label).disabled(self.exporting_image || frame.is_none()),
                            move |this: &mut Self, window, cx| {
                                this.export_image(area, scale, window, cx)
                            },
                        )
                    })
                    .collect();
                menu::submenu(label, icons::glyph::Image, scales)
            }),
        );
        transfer.push(menu::submenu("Export", icons::glyph::Image, exports));
        let snap = editor.snap();
        let settings = vec![
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
        ];
        menu::sections([
            navigation,
            if canvas_scope {
                vec![menu::submenu("Add", icons::math::Plus, creation)]
            } else {
                Vec::new()
            },
            selection,
            history,
            if canvas_scope { transfer } else { Vec::new() },
            if canvas_scope {
                vec![menu::submenu(
                    "Canvas options",
                    icons::account::Settings,
                    settings,
                )]
            } else {
                Vec::new()
            },
        ])
        .into_iter()
        .map(|(item, act)| {
            let action: menu::Act<Self> = Box::new(move |this, path, window, cx| {
                window.focus(&this.surface.read(cx).focus_handle(cx), cx);
                act(this, path, window, cx);
            });
            (item, action)
        })
        .collect()
    }
}
