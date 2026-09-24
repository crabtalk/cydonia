//! The appearance section: which of the three modes the app paints in.

use crate::{
    model::workspace::Workspace,
    view::settings::{self, SettingsWindow, Switch},
};
use artifact::board::View;
use bezel::{
    gpui::{AnyElement, Context, DragMoveEvent, Empty, div, prelude::*, px},
    theme::{
        TextStyle, Theme, Tint, Typeset,
        appearance::{self, AppearanceMode},
    },
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
    /// The whole page: the mode it paints in, then the colours it mixes, the
    /// size it reads at, and how the caret behaves in what it writes.
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
            .child(self.editor_group(cx))
            .children(self.boards_group(cx))
            .into_any_element()
    }

    /// One card row: what the setting is on the left, the control on the right.
    pub(super) fn theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let current = appearance::mode(cx);
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
                    .child(self.transparency_row(cx))
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
        let on = self.workspace.read(cx).indent_project_rows;
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Sidebar"))
            .child(
                theme.group_box().child(
                    self.switch_row(
                        Switch::new(
                            "indent-project-rows",
                            "Indent project rows",
                            "Inset items below each project heading by one icon width.",
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
                ),
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

    /// How the caret behaves — the editor's and every field's alike.
    fn editor_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Editor"))
            .child(
                theme
                    .group_box()
                    .child(self.cursor_row(cx))
                    .child(self.pages_row(cx))
                    .child(self.wrap_row(cx))
                    .child(self.highlight_row(cx)),
            )
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

    /// The colour `==text==` is washed in.
    pub(super) fn highlight_row(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::model::settings::Highlight;
        let theme = Theme::of(cx).clone();
        let current = self.workspace.read(cx).settings.appearance.highlight;
        theme
            .card_row(false)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(theme.row_title("Highlight colour")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .children(Highlight::ALL.into_iter().enumerate().map(|(ix, value)| {
                        div()
                            .id(("highlight-color", ix))
                            .size(px(18.))
                            .rounded_full()
                            .cursor_pointer()
                            .bg(crate::view::article::highlight_solid(value.color(), &theme))
                            .border_2()
                            .border_color(match current == value {
                                true => theme.accent,
                                false => bezel::gpui::transparent_black(),
                            })
                            .tooltip(move |window, cx| {
                                bezel::ui::tooltip::Tooltip::text(value.label(), window, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.workspace
                                    .update(cx, |workspace, cx| workspace.set_highlight(value, cx));
                                cx.notify();
                            }))
                    })),
            )
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
            ),
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
        self.tint_row(
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

    pub(super) fn intensity_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let tint = self.workspace.read(cx).tint;
        self.tint_row(
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

    fn tint_row(
        &self,
        id: &'static str,
        title: &'static str,
        note: &'static str,
        fraction: f32,
        set: impl Fn(&mut Workspace, f32, &mut Context<Workspace>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        theme
            .card_row(false)
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
