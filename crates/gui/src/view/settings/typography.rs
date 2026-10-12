//! Saved base sizes and the families they are set in. Keyboard zoom stays
//! separate, so resetting zoom always has a configured size to return to.

use crate::{
    model::{fonts, settings as config},
    view::settings::{SettingsWindow, group, labelled},
};
use bezel::{
    gpui::{AnyElement, Context, Entity, SharedString, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        combobox::{Combobox, ComboboxEvent},
        widgets::{Buttons, Scaffolding},
    },
};

/// The three faces text is set in: each has a family picker and a size.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Face {
    Interface,
    Article,
    Mono,
}

impl Face {
    fn title(self) -> &'static str {
        match self {
            Self::Interface => "Interface font",
            Self::Article => "Article font",
            Self::Mono => "Monospace font",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Interface => "Menus, controls, and the rest of the interface.",
            Self::Article => "Articles and transcripts — anything set as a document.",
            Self::Mono => "Terminals, code blocks, diffs, and file source.",
        }
    }

    /// What the first row is called: the answer for somebody who has not
    /// picked one.
    fn unset(self) -> &'static str {
        match self {
            Self::Interface | Self::Mono => "System",
            Self::Article => "Same as interface",
        }
    }
}

/// A searchable list of installed families, and what each row stands for.
/// `None` is [`SYSTEM`]; the rest are families the text system reported.
pub(super) struct FamilyPicker {
    face: Face,
    choices: Vec<Option<SharedString>>,
    combobox: Entity<Combobox>,
}

impl FamilyPicker {
    /// Build the picker for `face`, selected on `current`.
    ///
    /// A family the machine does not have still gets a row: it may have been
    /// typed into `settings.toml`, or come from a machine that had it, and
    /// dropping it would silently rewrite the setting on the next pick.
    pub(super) fn new(
        face: Face,
        current: Option<SharedString>,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let mut names: Vec<SharedString> = fonts::installed(cx)
            .into_iter()
            .filter(|family| face != Face::Mono || family.mono)
            .map(|family| family.name)
            .collect();
        if let Some(current) = current.clone()
            && !names.contains(&current)
        {
            names.push(current);
        }
        names.sort();
        let mut choices = vec![None];
        choices.extend(names.into_iter().map(Some));
        let selected = choices
            .iter()
            .position(|choice| *choice == current)
            .unwrap_or(0);
        let items = choices
            .iter()
            .map(|choice| choice.clone().unwrap_or_else(|| face.unset().into()))
            .collect();
        let combobox = cx.new(|cx| Combobox::new(items, face.unset(), cx).with_selection(selected));
        cx.subscribe(&combobox, move |this: &mut SettingsWindow, _, event, cx| {
            let ComboboxEvent::Selected(item) = event;
            let picker = match face {
                Face::Interface => &this.interface_font,
                Face::Article => &this.article_font,
                Face::Mono => &this.mono_font,
            };
            let Some(chosen) = picker.choices.get(*item).cloned() else {
                return;
            };
            this.workspace.update(cx, |workspace, cx| {
                let mut families = workspace.fonts.clone();
                match face {
                    Face::Interface => families.sans = chosen,
                    Face::Article => families.body = chosen,
                    Face::Mono => families.mono = chosen,
                }
                workspace.set_fonts(families, cx);
            });
            cx.notify();
        })
        .detach();
        Self {
            face,
            choices,
            combobox,
        }
    }
}

impl Face {
    // ── the size each face is set at ─────────────────────────────

    fn size_key(self) -> &'static str {
        match self {
            Self::Interface => "ui-font",
            Self::Article => "article-font",
            Self::Mono => "mono-font",
        }
    }

    fn size_title(self) -> &'static str {
        match self {
            Self::Interface => "Interface font size",
            Self::Article => "Article font size",
            Self::Mono => "Monospace font size",
        }
    }

    fn size_description(self) -> String {
        let zoom = || {
            format!(
                "{} / {} zooms; {} resets.",
                bezel::ui::keys::printed("secondary-="),
                bezel::ui::keys::printed("secondary--"),
                bezel::ui::keys::printed("secondary-0"),
            )
        };
        match self {
            Self::Interface => "Sizes for menus, controls, and the rest of the interface.".into(),
            Self::Article => format!("Default for articles. {}", zoom()),
            Self::Mono => format!(
                "Default for terminals, file source and previews. {}",
                zoom()
            ),
        }
    }

    fn size_range(self) -> (f32, f32) {
        match self {
            Self::Interface => config::TEXT_SIZE,
            _ => config::CONTENT_TEXT_SIZE,
        }
    }
}

impl SettingsWindow {
    pub(super) fn typography_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let sizes = [
            workspace.text_size,
            workspace.article_font_size(),
            workspace.mono_font_size,
        ];
        group(&theme, "Typography")
            .child(
                theme.group_box().children(
                    [Face::Interface, Face::Article, Face::Mono]
                        .into_iter()
                        .zip(sizes)
                        .enumerate()
                        .map(|(ix, (font, size))| {
                            theme
                                .card_row(ix == 0)
                                .child(labelled(
                                    &theme,
                                    font.size_title(),
                                    font.size_description(),
                                    false,
                                ))
                                .child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .rounded(px(Theme::button_radius()))
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.input_bg)
                                        .child(self.font_step(font, size, -1., cx))
                                        .child(
                                            div()
                                                .w(px(34.))
                                                .flex()
                                                .justify_center()
                                                .text_style(TextStyle::Callout)
                                                .text_color(theme.text)
                                                .child(format!("{size:.0}")),
                                        )
                                        .child(self.font_step(font, size, 1., cx)),
                                )
                        }),
                ),
            )
            .into_any_element()
    }

    /// The families the two faces are set in. A picker rather than a field:
    /// what the text system will resolve is a list, and a typo in a field is
    /// a family silently falling back to the one it replaced.
    pub(super) fn families_group(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        group(&theme, "Font families")
            .child(
                theme.group_box().children(
                    [&self.interface_font, &self.article_font, &self.mono_font]
                        .into_iter()
                        .enumerate()
                        .map(|(ix, picker)| {
                            theme
                                .card_row(ix == 0)
                                .child(labelled(
                                    &theme,
                                    picker.face.title(),
                                    picker.face.description(),
                                    false,
                                ))
                                .child(div().flex_none().w(px(220.)).child(picker.combobox.clone()))
                        }),
                ),
            )
            .into_any_element()
    }

    fn font_step(&self, font: Face, size: f32, by: f32, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (min, max) = font.size_range();
        let next = (size + by).clamp(min, max);
        theme
            .ghost((font.size_key(), usize::from(by > 0.)))
            .px(px(8.))
            .py(px(3.))
            .text_style(TextStyle::Callout)
            .text_color(if next == size {
                theme.text_faint
            } else {
                theme.text
            })
            .child(if by < 0. { "−" } else { "+" })
            .on_click(cx.listener(move |this, _, _, cx| {
                if next == size {
                    return;
                }
                this.workspace.update(cx, |workspace, cx| match font {
                    Face::Interface => workspace.set_text_size(next, cx),
                    Face::Article => workspace.set_article_font_size(next, cx),
                    Face::Mono => workspace.set_mono_font_size(next, cx),
                });
                cx.notify();
            }))
            .into_any_element()
    }
}
