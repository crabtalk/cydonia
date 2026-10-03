//! The appearance section: which of the three modes the app paints in.

use crate::model::settings::{Highlight, Paint};
use crate::{
    model::workspace::Workspace,
    view::settings::{self, SettingsWindow, Switch},
};
use artifact::board::View;
use bezel::theme::AppExt as _;
use bezel::ui::color::Swatch;
use bezel::{
    gpui::{AnyElement, Context, DragMoveEvent, Empty, div, prelude::*, px},
    theme::{TextStyle, Theme, Tint, Typeset, appearance::AppearanceMode},
    ui::widgets::{self, Controls, Scaffolding, SliderDrag},
};

/// The tint's ceiling: Slate's chroma, the most coloured of the five neutrals
/// bezel ships in `BASE_COLORS`. Past it the greys stop reading as greys.
const CHROMA_MAX: f32 = 0.046;

/// A full turn of oklch hue.
const HUE_MAX: f32 = 360.;

/// How wide a slider sits in its row.
const SLIDER_WIDTH: f32 = 160.;

const MODES: [AppearanceMode; 3] = [
    AppearanceMode::System,
    AppearanceMode::Light,
    AppearanceMode::Dark,
];

impl SettingsWindow {
    /// The whole page: the mode it paints in, then the colours it mixes and the
    /// size it reads at.
    /// Typography is a group here rather than a section of its own — a size is
    /// a question about appearance.
    pub(super) fn appearance_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(theme.group_box().child(self.theme_row(cx)))
            .child(self.colors_group(cx))
            .child(self.typography_group(cx))
            .child(self.families_group(cx))
            .child(self.sidebar_group(cx))
            .child(self.scrollbars_group(cx))
            .children(self.boards_group(cx))
            .into_any_element()
    }

    /// One card row: what the setting is on the left, the control on the right.
    pub(super) fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let current = cx.appearance_mode();
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Theme"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child("Follow the system, or pick one."),
                    ),
            )
            .child(
                // A segmented control rather than a select: three options that
                // all fit are worth showing at once.
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(Theme::button_radius()))
                    .border_1()
                    .border_color(theme.border)
                    .children(MODES.into_iter().enumerate().map(|(ix, mode)| {
                        let selected = mode == current;
                        div()
                            .id(("appearance", ix))
                            .px(px(10.))
                            .py(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .text_style(TextStyle::Callout)
                            .cursor_pointer()
                            .when(selected, |el| {
                                el.bg(theme.element_active).text_color(theme.text)
                            })
                            .when(!selected, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.element_hover))
                            })
                            .child(mode.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.set_appearance(mode, cx));
                                cx.notify();
                            }))
                    })),
            )
    }

    /// What the greys are mixed from, and whether they are see-through.
    fn colors_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Colors"))
            .child(
                theme
                    .group_box()
                    .children(self.vibrancy_row(cx))
                    .children(self.blur_row(cx))
                    .child(self.hue_row(cx))
                    .child(self.intensity_row(cx)),
            )
            .into_any_element()
    }

    fn scrollbars_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Scrollbars"))
            .child(
                theme
                    .group_box()
                    .child(self.scrollbars_row(true, cx))
                    .child(self.scrollbars_row(false, cx)),
            )
            .into_any_element()
    }

    fn scrollbars_row(&self, sidebar: bool, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::Scrollbars;
        let theme = Theme::of(cx).clone();
        let look = self.workspace.read(cx).settings.appearance.clone();
        let current = if sidebar {
            look.sidebar_scrollbars
        } else {
            look.scrollbars
        };
        theme
            .card_row(sidebar)
            .child(div().flex_1().min_w_0().child(theme.row_title(if sidebar {
                "Sidebar scrollbars"
            } else {
                "Content scrollbars"
            })))
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .children(Scrollbars::ALL.into_iter().enumerate().map(|(ix, value)| {
                        div()
                            .id((
                                if sidebar {
                                    "sidebar-scrollbars"
                                } else {
                                    "content-scrollbars"
                                },
                                ix,
                            ))
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(Theme::control_radius()))
                            .text_style(TextStyle::Callout)
                            .cursor_pointer()
                            .when(current == value, |el| el.bg(theme.element_active))
                            .when(current != value, |el| {
                                el.text_color(theme.text_muted)
                                    .hover(|el| el.bg(theme.element_hover))
                            })
                            .child(value.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_scrollbars(value, sidebar, cx)
                                });
                                cx.notify();
                            }))
                    })),
            )
            .into_any_element()
    }

    fn sidebar_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let on = workspace.indent_project_rows;
        let fits = workspace.settings.appearance.settings_sidebar_fits;
        let lights = workspace.settings.appearance.traffic_lights;
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Sidebar"))
            .child(
                theme
                    .group_box()
                    .child(
                        self.switch_row(
                            Switch::new(
                                "indent-project-rows",
                                "Indent sidebar rows",
                                "Inset items below each project and space heading by one icon width.",
                                on,
                            )
                            .first(true),
                            cx,
                            move |this, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_indent_project_rows(!on, cx);
                                });
                            },
                        ),
                    )
                    .child(self.switch_row(
                        Switch::new(
                            "settings-sidebar-fits",
                            "Fit settings sidebar",
                            "Size this window's sidebar to its widest section.",
                            fits,
                        ),
                        cx,
                        move |this, cx| {
                            this.workspace.update(cx, |workspace, cx| {
                                workspace.set_settings_sidebar_fits(!fits, cx);
                            });
                        },
                    ))
                    .when(!cfg!(target_os = "macos"), |group| {
                        group.child(self.switch_row(
                            Switch::new(
                                "traffic-lights",
                                "Traffic light window buttons",
                                "Close, minimise and maximise as coloured dots on the left.",
                                lights,
                            ),
                            cx,
                            move |this, cx| {
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_traffic_lights(!lights, cx);
                                });
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    /// How a new board is laid out. The pill at the foot of a board is what
    /// moves one already made — every board carries its own answer, so this
    /// only says what a board starts as.
    ///
    /// Nothing at all with boards switched off: a default for a pane that
    /// cannot be reached is a switch that does nothing.
    fn boards_group(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        if !workspace.settings.features.boards {
            return None;
        }
        let on = workspace.board_view == View::List;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(settings::LABEL_GAP))
                .child(theme.field_label("Boards"))
                .child(
                    theme.group_box().child(
                        self.switch_row(
                            Switch::new(
                                "board-view",
                                "New boards in list view",
                                "Start a board as one list down rather than lanes across.",
                                on,
                            )
                            .first(true),
                            cx,
                            move |this, cx| {
                                let view = match on {
                                    true => View::Lanes,
                                    false => View::List,
                                };
                                this.workspace.update(cx, |workspace, cx| {
                                    workspace.set_default_board_view(view, cx);
                                });
                            },
                        ),
                    ),
                )
                .into_any_element(),
        )
    }

    /// The Editor section. The Cursor group's rows reach every text field, not
    /// the editor alone.
    pub(super) fn editor_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let group = |label: &'static str, rows: Vec<AnyElement>| {
            div()
                .flex()
                .flex_col()
                .gap(px(settings::LABEL_GAP))
                .child(theme.field_label(label))
                .child(theme.group_box().children(rows))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(settings::GROUP_GAP))
            .child(group(
                "Cursor",
                vec![
                    self.cursor_row(cx),
                    self.caret_shape_row(cx),
                    self.caret_height_row(cx),
                    self.caret_row(cx),
                ],
            ))
            .child(group("Layout", vec![self.pages_row(cx), self.wrap_row(cx)]))
            .child(group(
                "Colours",
                vec![
                    self.highlight_row(cx),
                    self.selection_row(cx),
                    self.find_row(cx),
                ],
            ))
            .child(group(
                "Pasting",
                vec![self.source_paste_row(cx), self.download_web_row(cx)],
            ))
            .into_any_element()
    }

    /// What a line too long for a code block does.
    ///
    /// Every document at once, an article's fences and a transcript's alike:
    /// the renderer takes one answer for the app. Off is what an editor
    /// usually does, and the cost of it here is that nothing scrolls a fence
    /// back to a caret typed off its right edge — the page follows the caret
    /// down, but a block's own sideways scroll is the reader's to drag.
    pub(super) fn wrap_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).wrap_code;
        self.switch_row(
            Switch::new(
                "wrap-code",
                "Wrap long lines in code",
                "Off scrolls a long line sideways inside the block instead.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_wrap_code(!on, cx));
            },
        )
    }

    /// Whether plain-text mode takes a pasted picture as an image line.
    fn source_paste_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.paste_images_in_source;
        self.switch_row(
            Switch::new(
                "paste-images-in-source",
                "Paste pictures in plain text",
                "A pasted picture, file or picture link goes in as an image line. Off pastes text.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_paste_images_in_source(!on, cx)
                });
            },
        )
    }

    /// Whether a picture's pasted web address is downloaded into the article.
    fn download_web_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.download_web_images;
        self.switch_row(
            Switch::new(
                "download-web-images",
                "Download web pictures",
                "Save a copy of a pasted picture link into the article. Off keeps the web address.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_download_web_images(!on, cx)
                });
            },
        )
    }

    /// The colour `==text==` is washed in.
    pub(super) fn highlight_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.workspace.read(cx).settings.appearance.highlight;
        let paints = Highlight::ALL.into_iter().map(Paint::Named).collect();
        self.color_row(
            true,
            "highlight-color",
            "Highlight colour",
            Some(current),
            paints,
            None,
            true,
            cx,
            |workspace, value, cx| {
                if let Some(value) = value {
                    workspace.set_highlight(value, cx);
                }
            },
        )
    }

    /// The colour selected text is washed in, or the palette's own.
    fn selection_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.selection;
        let system = Theme::for_appearance(theme.appearance).selection;
        self.wash_row(
            "selection-color",
            "Selection colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_selection(value, cx),
        )
    }

    fn caret_shape_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::CaretShape;
        use bezel::ui::popover;
        const ID: &str = "caret-shape";
        let theme = Theme::of(cx).clone();
        let appearance = &self.workspace.read(cx).settings.appearance;
        let current = appearance.caret_shape;
        let caret = appearance
            .caret
            .map_or(theme.caret, |paint| paint.solid(&theme));
        let trigger = popover::menu_trigger_matching(
            theme
                .select_trigger_with(
                    Some(div().text_color(caret).child(current.glyph())),
                    current.label(),
                )
                .gap(px(8.))
                .id(ID)
                .relative(),
            |this: &mut Self| &mut this.picker,
            |open| *open == ID,
            |_| ID,
            cx,
        );
        let card = (self.picker.get() == Some(&ID)).then(|| {
            let rows = CaretShape::ALL.into_iter().enumerate().map(|(ix, shape)| {
                popover::menu_row(&theme, shape == current, None)
                    .id((ID, ix))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child(div().text_color(caret).child(shape.glyph()))
                    .child(shape.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.set_caret_shape(shape, cx));
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                "caret-shape-menu",
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .children(rows),
                    |this: &mut Self| &mut this.picker,
                    cx,
                )
                .into_any_element(),
                self.picker.closing_since(),
            )
        });
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(theme.row_title("Cursor shape")),
            )
            .child(trigger.children(card))
            .into_any_element()
    }

    /// Whether a block caret fills its line or stands as tall as the text.
    fn caret_height_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::CaretHeight;
        let on = self.workspace.read(cx).settings.appearance.caret_height == CaretHeight::Line;
        self.switch_row(
            Switch::new(
                "caret-height",
                "Fill the line",
                "Off sizes a block cursor to the text instead of its line.",
                on,
            ),
            cx,
            move |this, cx| {
                let height = match on {
                    true => CaretHeight::Text,
                    false => CaretHeight::Line,
                };
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_caret_height(height, cx));
            },
        )
    }

    /// The caret's colour, or the palette's own.
    fn caret_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.caret;
        let system = Theme::for_appearance(theme.appearance).caret;
        let paints = Highlight::ALL.into_iter().map(Paint::Named).collect();
        self.color_row(
            false,
            "caret-color",
            "Cursor colour",
            current,
            paints,
            Some(system),
            false,
            cx,
            |workspace, value, cx| workspace.set_caret(value, cx),
        )
    }

    /// The colour find matches are washed in, or the accent.
    fn find_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.search;
        let system = markdown::default_find(&theme).1;
        self.wash_row(
            "search-color",
            "Search results colour",
            current,
            system,
            cx,
            |workspace, value, cx| workspace.set_search(value, cx),
        )
    }

    /// bezel's preset colours in rows of six, the colour picker, and Default
    /// under them. A colour written by hand that is not among them rings
    /// nothing.
    #[allow(clippy::too_many_arguments)]
    fn wash_row(
        &self,
        id: &'static str,
        title: &'static str,
        current: Option<Paint>,
        system: bezel::gpui::Hsla,
        cx: &mut Context<Self>,
        set: fn(&mut Workspace, Option<Paint>, &mut Context<Workspace>),
    ) -> AnyElement {
        let paints = Highlight::ALL.into_iter().map(Paint::Named).collect();
        self.color_row(
            false,
            id,
            title,
            current,
            paints,
            Some(system),
            true,
            cx,
            set,
        )
    }

    /// A row showing one colour, which opens `paints` and a colour picker
    /// under it to pick another. With `system`, the popover ends
    /// in a Default item that sets `None`, and `None` shows `system`.
    /// With `wash`, every colour shows as [`Paint::wash`] paints it, and the
    /// picker sets alpha too.
    #[allow(clippy::too_many_arguments)]
    fn color_row(
        &self,
        first: bool,
        id: &'static str,
        title: &'static str,
        current: Option<Paint>,
        paints: Vec<Paint>,
        system: Option<bezel::gpui::Hsla>,
        wash: bool,
        cx: &mut Context<Self>,
        set: fn(&mut Workspace, Option<Paint>, &mut Context<Workspace>),
    ) -> AnyElement {
        use bezel::ui::color::{ColorPicker, ColorPickerEvent};
        use bezel::ui::popover;
        let theme = Theme::of(cx).clone();
        let painted = move |paint: Paint, theme: &Theme| match wash {
            true => paint.wash(theme),
            false => paint.solid(theme),
        };
        let shown = current
            .map(|paint| painted(paint, &theme))
            .or(system)
            .unwrap_or_default();
        let selected =
            current.and_then(|current| paints.iter().position(|paint| *paint == current));
        // Down lands before the trigger's click opens the card, so the picker
        // is in place, at the colour shown, by the card's first frame.
        let ready = cx.listener(move |this, _: &bezel::gpui::MouseDownEvent, _, cx| {
            if let Some(custom) = this.custom.as_ref().filter(|custom| custom.id == id) {
                custom
                    .picker
                    .update(cx, |picker, cx| picker.set_color(shown, cx));
                return;
            }
            let picker = cx.new(|cx| ColorPicker::new(shown, wash, cx));
            let changed = cx.subscribe(&picker, move |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Changed(color) = *event;
                let paint = Paint::from_hsla(color, wash);
                this.workspace
                    .update(cx, |workspace, cx| set(workspace, Some(paint), cx));
            });
            this.custom = Some(settings::CustomColor {
                id,
                picker,
                _changed: changed,
            });
        });
        let well = popover::menu_trigger_matching(
            theme
                .color_well(shown)
                .id(id)
                .relative()
                .on_mouse_down(bezel::gpui::MouseButton::Left, ready),
            |this: &mut Self| &mut this.picker,
            move |open| *open == id,
            move |_| id,
            cx,
        );
        let card = (self.picker.get() == Some(&id)).then(|| {
            let swatches: Vec<Swatch> = paints
                .iter()
                .map(|paint| Swatch::fixed(paint.key(), painted(*paint, &theme)))
                .collect();
            let presets =
                theme.swatch_picker((id, 0usize), &swatches, selected, Some(SWATCH_COLUMNS), {
                    let paints = paints.clone();
                    cx.listener(move |this, ix: &usize, _, cx| {
                        let paint = paints[*ix];
                        this.workspace
                            .update(cx, |workspace, cx| set(workspace, Some(paint), cx));
                        if let Some(custom) = this.custom.as_ref().filter(|custom| custom.id == id)
                        {
                            let color = painted(paint, Theme::of(cx));
                            custom
                                .picker
                                .update(cx, |picker, cx| picker.set_color(color, cx));
                        }
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    })
                });
            let custom = self
                .custom
                .as_ref()
                .filter(|custom| custom.id == id)
                .map(|custom| custom.picker.clone());
            let reset = system.map(|_| {
                popover::menu_row(&theme, current.is_none(), None)
                    .id((id, 1usize))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child("Default")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| set(workspace, None, cx));
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                bezel::gpui::SharedString::from(format!("{id}-swatches")),
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .child(
                            // As wide as the preset grid; the picker fills it.
                            div()
                                .p(px(popover::MENU_ROW_INSET))
                                .flex()
                                .flex_col()
                                .gap(px(popover::MENU_ROW_INSET))
                                .child(presets)
                                .children(custom),
                        )
                        .children(reset),
                    |this: &mut Self| &mut this.picker,
                    cx,
                )
                .into_any_element(),
                self.picker.closing_since(),
            )
        });
        theme
            .card_row(first)
            .child(div().flex_1().min_w_0().child(theme.row_title(title)))
            .child(well.children(card))
            .into_any_element()
    }

    /// How wide a page is set when it has not been told otherwise.
    ///
    /// The default alone. A page's own `···` menu writes the measure into its
    /// `properties.toml`, and one written down there is the document's — it
    /// stays what its author made it whatever this switch says, which is what
    /// the menu's Use default width hands back.
    pub(super) fn pages_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).wide_pages;
        self.switch_row(
            Switch::new(
                "wide-pages",
                "Full width pages",
                "Set articles across the pane rather than in a reading column.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_wide_pages(!on, cx));
            },
        )
    }

    /// Whether the caret blinks. bezel holds the caret, so the switch sets it
    /// there rather than keeping a second copy of the answer.
    pub(super) fn cursor_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).cursor_blink;
        self.switch_row(
            Switch::new(
                "cursor-blink",
                "Blink the cursor",
                "Off holds the text caret lit while it has focus.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_cursor_blink(!on, cx));
            },
        )
    }

    /// The hue every grey carries, and how much of it. Two rows because they
    /// are two questions: a hue nobody can see is still the hue that returns
    /// when the intensity comes back up.
    pub(super) fn hue_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let tint = self.workspace.read(cx).tint;
        // First in its group where the frost rows are not shown.
        self.tint_row(
            !Theme::of(cx).vibrancy,
            "hue",
            "Hue",
            "Which hue the greys are mixed from.",
            tint.hue / HUE_MAX,
            move |workspace, fraction, cx| {
                let tint = Tint::new(fraction * HUE_MAX, workspace.tint.chroma);
                workspace.set_tint(tint, cx);
            },
            cx,
        )
    }

    /// How much the frost shows through. Nothing where the window is not
    /// frosted — opaque, or an appearance the palette does not frost.
    fn vibrancy_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::model::settings::VIBRANCY;
        if !Theme::of(cx).vibrancy {
            return None;
        }
        let (min, max) = VIBRANCY;
        let alpha = self.workspace.read(cx).settings.appearance.vibrancy;
        // Right is more see-through, so the slider runs against the alpha.
        Some(self.tint_row(
            true,
            "vibrancy",
            "Transparency",
            "How much of what is behind the window shows through.",
            (max - alpha) / (max - min),
            move |workspace, fraction, cx| {
                workspace.set_vibrancy(max - fraction * (max - min), cx);
            },
            cx,
        ))
    }

    /// How far what is behind the window is blurred. Shown with
    /// [`Self::vibrancy_row`].
    fn blur_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::model::settings::BLUR;
        if !Theme::of(cx).vibrancy {
            return None;
        }
        let (min, max) = BLUR;
        let blur = self.workspace.read(cx).settings.appearance.blur;
        Some(self.tint_row(
            false,
            "blur",
            "Blur",
            "How much what is behind the window is blurred.",
            (blur - min) / (max - min),
            move |workspace, fraction, cx| {
                workspace.set_blur(min + fraction * (max - min), cx);
            },
            cx,
        ))
    }

    pub(super) fn intensity_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let tint = self.workspace.read(cx).tint;
        self.tint_row(
            false,
            "intensity",
            "Intensity",
            "How much of that hue they carry. None is the shipped neutral.",
            tint.chroma / CHROMA_MAX,
            move |workspace, fraction, cx| {
                let tint = Tint::new(workspace.tint.hue, fraction * CHROMA_MAX);
                workspace.set_tint(tint, cx);
            },
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn tint_row(
        &self,
        first: bool,
        id: &'static str,
        title: &'static str,
        note: &'static str,
        fraction: f32,
        set: impl Fn(&mut Workspace, f32, &mut Context<Workspace>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(first)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title(title))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child(note),
                    ),
            )
            .child(
                div()
                    .id(id)
                    .flex_none()
                    .w(px(SLIDER_WIDTH))
                    .child(theme.slider(fraction))
                    .on_drag(SliderDrag(id.into()), |_, _, _, cx| cx.new(|_| Empty))
                    .on_drag_move(cx.listener(
                        move |this, event: &DragMoveEvent<SliderDrag>, _, cx| {
                            let Some(fraction) = widgets::slider_fraction(event, id, cx) else {
                                return;
                            };
                            this.workspace
                                .update(cx, |workspace, cx| set(workspace, fraction, cx));
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    }
}

/// Swatches a row of the colour popover holds.
const SWATCH_COLUMNS: usize = 6;
