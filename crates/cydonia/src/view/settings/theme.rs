//! The appearance section: which of the three modes the app paints in.

use crate::{
    model::workspace::Workspace,
    view::settings::{self, SettingsWindow},
};
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
            .child(self.editor_group(cx))
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

    /// How the caret behaves — the editor's and every field's alike.
    fn editor_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_col()
            .gap(px(settings::LABEL_GAP))
            .child(theme.field_label("Editor"))
            .child(theme.group_box().child(self.cursor_row(cx)))
            .into_any_element()
    }

    /// The app's own reduce-transparency switch, so the vibrancy can go without
    /// turning the system setting on for every other app.
    pub(super) fn transparency_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let on = self.workspace.read(cx).reduce_transparency;
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Reduce transparency"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child("Replace translucent surfaces with opaque backgrounds."),
                    ),
            )
            .child(
                div()
                    .id("reduce-transparency")
                    .cursor_pointer()
                    .child(theme.toggle(on))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace.update(cx, |workspace, cx| {
                            workspace.set_reduce_transparency(!on, cx)
                        });
                        cx.notify();
                    })),
            )
    }

    /// Whether the caret blinks. bezel holds the caret, so the switch sets it
    /// there rather than keeping a second copy of the answer.
    pub(super) fn cursor_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let on = self.workspace.read(cx).cursor_blink;
        theme
            .card_row(true)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(theme.row_title("Blink the cursor"))
                    .child(
                        div()
                            .mt(px(4.))
                            .text_style(TextStyle::Subheadline)
                            .text_color(theme.text_muted)
                            .child("Off holds the text caret lit while it has focus."),
                    ),
            )
            .child(
                div()
                    .id("cursor-blink")
                    .cursor_pointer()
                    .child(theme.toggle(on))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.set_cursor_blink(!on, cx));
                        cx.notify();
                    })),
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
