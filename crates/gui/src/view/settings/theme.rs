//! The appearance section: which of the three modes the app paints in.

use crate::model::settings::Paint;
use crate::{
    model::workspace::Workspace,
    view::settings::{self, SettingsWindow, Switch},
};
use artifact::board::View;
use bezel::theme::AppExt as _;
use bezel::ui::{AppExt as _, color::Swatch};
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
                    .when(cfg!(target_os = "macos"), |group| {
                        group.child(self.transparency_row(cx))
                    })
                    .children(self.vibrancy_row(cx))
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
                    )),
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
                vec![self.source_paste_row(cx), self.keep_pasted_row(cx)],
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
    fn keep_pasted_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).settings.keep_pasted_images;
        self.switch_row(
            Switch::new(
                "keep-pasted-images",
                "Save pasted pictures",
                "Download a pasted picture link into the article. Off keeps the link.",
                on,
            ),
            cx,
            move |this, cx| {
                this.workspace.update(cx, |workspace, cx| {
                    workspace.set_keep_pasted_images(!on, cx)
                });
            },
        )
    }

    /// The colour `==text==` is washed in.
    pub(super) fn highlight_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::Highlight;
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.highlight;
        let selected = Highlight::ALL.iter().position(|held| *held == current);
        self.color_row(
            true,
            "highlight-color",
            "Highlight colour",
            markdown::highlight_solid(current.color(), &theme),
            highlight_swatches(),
            selected,
            None,
            cx,
            |this, ix, cx| {
                if let Some(value) = ix.map(|ix| Highlight::ALL[ix]) {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.set_highlight(value, cx));
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

    /// The caret's colour, or the palette's own.
    fn caret_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.caret;
        let system = Theme::for_appearance(theme.appearance).caret;
        self.wash_row(
            "caret-color",
            "Cursor colour",
            current,
            system,
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

    /// bezel's preset colours in rows of six, and Default under them. A
    /// colour written by hand that is not among them rings nothing.
    fn wash_row(
        &self,
        id: &'static str,
        title: &'static str,
        current: Option<Paint>,
        system: bezel::gpui::Hsla,
        cx: &mut Context<Self>,
        set: fn(&mut Workspace, Option<Paint>, &mut Context<Workspace>),
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let swatches: Vec<Swatch> = cx.color_swatches().iter().cloned().collect();
        let paints: Vec<Paint> = swatches
            .iter()
            .map(|swatch| Paint::from_hsla(swatch.resolve(&theme)))
            .collect();
        let selected =
            current.and_then(|current| paints.iter().position(|paint| *paint == current));
        let shown = current.map_or(system, |paint| paint.solid(&theme));
        self.color_row(
            false,
            id,
            title,
            shown,
            swatches,
            selected,
            Some(Reset {
                on: current.is_none(),
            }),
            cx,
            move |this, ix, cx| {
                let value = ix.map(|ix| paints[ix]);
                this.workspace
                    .update(cx, |workspace, cx| set(workspace, value, cx));
            },
        )
    }

    /// A row showing one colour, which opens `swatches` under it to pick
    /// another. `pick` hears a swatch's index, or `None` for the Default item
    /// when there is one.
    #[allow(clippy::too_many_arguments)]
    fn color_row(
        &self,
        first: bool,
        id: &'static str,
        title: &'static str,
        shown: bezel::gpui::Hsla,
        swatches: Vec<Swatch>,
        selected: Option<usize>,
        default: Option<Reset>,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, Option<usize>, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        use bezel::ui::popover;
        let theme = Theme::of(cx).clone();
        let well = popover::menu_trigger_matching(
            theme.color_well(shown).id(id).relative(),
            |this: &mut Self| &mut this.picker,
            move |open| *open == id,
            move |_| id,
            cx,
        );
        let pick = std::rc::Rc::new(pick);
        let card = (self.picker.get() == Some(&id)).then(|| {
            let columns = default.is_some().then_some(SWATCH_COLUMNS);
            let picker = theme.swatch_picker((id, 0usize), &swatches, selected, columns, {
                let pick = pick.clone();
                cx.listener(move |this, ix: &usize, _, cx| {
                    pick(this, Some(*ix), cx);
                    popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                })
            });
            let reset = default.map(|Reset { on }| {
                let pick = pick.clone();
                popover::menu_row(&theme, on, None)
                    .id((id, 1usize))
                    .mt(px(4.))
                    .cursor_pointer()
                    .hover(|row| row.bg(theme.element_hover))
                    .child("Default")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        pick(this, None, cx);
                        popover::close_popup(this, cx, |this: &mut Self| &mut this.picker);
                    }))
            });
            popover::anchored_menu_below_end(
                bezel::gpui::SharedString::from(format!("{id}-swatches")),
                popover::dismiss_on_out(
                    popover::popover_card(&theme)
                        .flex()
                        .flex_col()
                        .child(picker)
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

    /// The app's own reduce-transparency switch, separate from the system one.
    ///
    /// Live in both appearances. The window's frost is dark's alone —
    /// [`crate::model::workspace::vibrancy`] never returns `Vibrancy::On` —
    /// but glass is [`crate::model::workspace::glass`]'s separate answer and
    /// a light window carries it, so there is something here to turn off.
    pub(super) fn transparency_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.workspace.read(cx).opaque.unwrap_or(false);
        self.switch_row(
            Switch::new(
                "reduce-transparency",
                "Reduce transparency",
                "Replace translucent surfaces with opaque backgrounds.",
                on,
            )
            .first(true),
            cx,
            move |this, cx| {
                this.workspace
                    .update(cx, |workspace, cx| workspace.set_opaque(!on, cx));
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
        // First in its group where the transparency switch is not shown.
        self.tint_row(
            !cfg!(target_os = "macos"),
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
            false,
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

/// The highlight colours as swatches, each with its light and dark value.
fn highlight_swatches() -> Vec<Swatch> {
    use crate::model::settings::Highlight;
    use bezel::theme::Appearance;
    let light = Theme::for_appearance(Appearance::Light);
    let dark = Theme::for_appearance(Appearance::Dark);
    Highlight::ALL
        .into_iter()
        .map(|named| {
            Swatch::new(
                named.label(),
                markdown::highlight_solid(named.color(), &light),
                markdown::highlight_solid(named.color(), &dark),
            )
        })
        .collect()
}

/// Swatches a row of the colour popover holds.
const SWATCH_COLUMNS: usize = 6;

/// The popover's Default item, and whether it is the colour in use.
#[derive(Clone, Copy)]
struct Reset {
    on: bool,
}
