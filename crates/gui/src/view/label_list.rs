//! The labels modal: one project's labels, each with its colour, its
//! description and how many entries carry it, made, edited and deleted in
//! place. Opened from the project heading's menu.

use crate::{
    model::settings::{Highlight, Paint},
    view::{
        component::{
            color,
            menu::{self, Menu},
        },
        root::Cydonia,
    },
};
use artifact::label::Label;
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, KeyBinding, SharedString,
        Subscription, Window, actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        color::{ColorPicker, ColorPickerEvent},
        icons,
        input::{FieldEvent, TextField},
        menu::Item,
        multi_select, popover, scroll,
        widgets::{ButtonStyle, Buttons, Content, Controls, Scaffolding as _},
    },
};
use std::path::PathBuf;

actions!(cydonia_labels, [CommitLabel, DismissLabel]);

/// Claimed on the form's fields, so `enter` saves it and `escape` puts it
/// away.
const FORM_CONTEXT: &str = "CydoniaLabelForm";

/// How wide the form's labels run, so its fields start on one edge.
const LABEL_WIDTH: f32 = 76.;

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", CommitLabel, Some(FORM_CONTEXT)),
        KeyBinding::new("escape", DismissLabel, Some(FORM_CONTEXT)),
    ]
}

/// The modal, while it is up.
pub(crate) struct Labelling {
    /// Whose labels, by path: the modal outlives a re-read of the projects.
    project: PathBuf,
    form: Option<Form>,
}

/// A label open for editing, or one being made. Buffered: nothing is written
/// until Save, and dismissing discards.
struct Form {
    /// The name it is filed under now; `None` for a new one.
    was: Option<String>,
    name: Entity<TextField>,
    description: Entity<TextField>,
    /// `None` paints it the colour read off its name.
    color: Option<Paint>,
    picker: Entity<ColorPicker>,
    /// What was wrong last time. Keeps the form open.
    error: Option<SharedString>,
    _changed: Subscription,
    _typed: Subscription,
}

/// One of the form's fields, holding what it opens with.
fn seed(content: String, placeholder: &'static str, cx: &mut App) -> Entity<TextField> {
    let field = cx.new(|cx| {
        TextField::new(cx)
            .with_key_context(FORM_CONTEXT)
            .with_placeholder(placeholder)
    });
    field.update(cx, |field, cx| field.set_content(content, cx));
    field
}

fn presets() -> Vec<Paint> {
    Highlight::ALL.into_iter().map(Paint::Named).collect()
}

impl Cydonia {
    /// Put the modal up on the labels of the project at `project`.
    pub(crate) fn open_labels(&mut self, project: PathBuf, cx: &mut Context<Self>) {
        self.menu = None;
        self.labelling = Some(Labelling {
            project,
            form: None,
        });
        cx.notify();
    }

    /// Where the modal's project is in the open list, while it is open.
    fn labelling_at(&self, cx: &App) -> Option<usize> {
        let labelling = self.labelling.as_ref()?;
        self.workspace.read(cx).project_at(&labelling.project)
    }

    /// Open the form on `name` as the project files it, or on a new label.
    fn edit_label(&mut self, name: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.labelling_at(cx) else {
            return;
        };
        let held = name
            .as_ref()
            .and_then(|name| {
                self.workspace.read(cx).projects[at]
                    .labels
                    .get(name)
                    .cloned()
            })
            .unwrap_or_default();
        let color = held.color.as_deref().and_then(Paint::parse);
        let theme = Theme::of(cx).clone();
        let shown = match (color, &name) {
            (Some(paint), _) => paint.solid(&theme),
            (None, Some(name)) => multi_select::tint(&theme, name),
            (None, None) => theme.accent,
        };
        let picker = cx.new(|cx| ColorPicker::new(shown, false, cx));
        let changed = cx.subscribe(&picker, |this, _, event: &ColorPickerEvent, cx| {
            let ColorPickerEvent::Changed(color) = *event;
            if let Some(form) = this.labelling.as_mut().and_then(|it| it.form.as_mut()) {
                form.color = Some(Paint::from_hsla(color));
                cx.notify();
            }
        });
        let field = seed(name.clone().unwrap_or_default(), "name", cx);
        // The preview chip is drawn from the root's render, so what is typed
        // has to reach the root.
        let typed = cx.subscribe(&field, |_, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                cx.notify();
            }
        });
        let description = seed(held.description.unwrap_or_default(), "what it is for", cx);
        window.focus(&field.read(cx).focus_handle(cx), cx);
        if let Some(labelling) = &mut self.labelling {
            labelling.form = Some(Form {
                was: name,
                name: field,
                description,
                color,
                picker,
                error: None,
                _changed: changed,
                _typed: typed,
            });
        }
        cx.notify();
    }

    pub(crate) fn commit_label(&mut self, _: &CommitLabel, _: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self.labelling_at(cx) else {
            return;
        };
        let Some(form) = self.labelling.as_ref().and_then(|it| it.form.as_ref()) else {
            return;
        };
        let was = form.was.clone();
        let name = form.name.read(cx).content().to_string();
        let description = form.description.read(cx).content().trim().to_owned();
        let label = Label {
            color: form.color.map(Paint::key),
            description: (!description.is_empty()).then_some(description),
        };
        let filed = self.workspace.update(cx, |workspace, cx| {
            workspace.save_label(at, was.as_deref(), &name, label, cx)
        });
        if let (Ok(name), Some(was)) = (&filed, &was) {
            self.follow_label_rename(was, name);
        }
        if let Some(labelling) = &mut self.labelling {
            match filed {
                Ok(_) => labelling.form = None,
                Err(why) => {
                    if let Some(form) = &mut labelling.form {
                        form.error = Some(why.into());
                    }
                }
            }
        }
        cx.notify();
    }

    /// Put the form away, or the modal where there is none.
    pub(crate) fn dismiss_label(
        &mut self,
        _: &DismissLabel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(labelling) = &mut self.labelling
            && labelling.form.take().is_none()
        {
            self.labelling = None;
        }
        cx.notify();
    }

    fn set_form_color(&mut self, color: Option<Paint>, cx: &mut Context<Self>) {
        let theme = Theme::of(cx).clone();
        let Some(form) = self.labelling.as_mut().and_then(|it| it.form.as_mut()) else {
            return;
        };
        form.color = color;
        self.menu = None;
        if let Some(paint) = color {
            form.picker
                .update(cx, |picker, cx| picker.set_color(paint.solid(&theme), cx));
        }
        cx.notify();
    }

    /// The modal, over a scrim that takes the press dismissing it — the shape
    /// the delete question is asked in.
    pub(crate) fn labels_modal(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let labelling = self.labelling.as_ref()?;
        let at = self.labelling_at(cx)?;
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let counts = workspace.labels_in(at);
        let project = workspace.projects[at].name();
        let editing = labelling.form.as_ref().map(|form| form.was.clone());
        let rows: Vec<AnyElement> = counts
            .into_iter()
            .enumerate()
            .map(
                |(ix, (name, count))| match editing == Some(Some(name.clone())) {
                    true => self.label_form(cx),
                    false => self.label_row(ix, name, count, at, cx),
                },
            )
            .collect();
        let empty = rows.is_empty() && editing != Some(None);
        Some(
            div()
                .id("labels-scrim")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim())
                .child(bezel::ui::cover::cover())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.labelling = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("labels-dialog")
                        .w(px(560.))
                        .max_h(px(560.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .p(px(20.))
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        // The press that answers must not reach the scrim.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(8.))
                                .child(theme.row_title("Labels"))
                                .child(self.labels_project(project, window, cx))
                                .child(div().flex_1())
                                .child(
                                    theme
                                        .button("New label", ButtonStyle::Prominent, None)
                                        .id("labels-new")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.edit_label(None, window, cx)
                                        })),
                                ),
                        )
                        .children((editing == Some(None)).then(|| self.label_form(cx)))
                        .child(
                            div()
                                .id("labels-rows")
                                .flex_1()
                                .min_h_0()
                                .map(|el| scroll::scrolls(el, scroll::Axes::Vertical))
                                .flex()
                                .flex_col()
                                .children(rows)
                                .when(empty, |list| {
                                    list.child(
                                        div()
                                            .py(px(24.))
                                            .flex()
                                            .justify_center()
                                            .text_style(TextStyle::Callout)
                                            .text_color(theme.text_faint)
                                            .child("No labels yet"),
                                    )
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The project the modal is on, opening the open ones to pick another.
    fn labels_project(
        &self,
        project: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let menu = Menu::Labelling;
        let trigger = theme
            .select_trigger(project)
            .flex_none()
            .id("labels-project")
            .relative()
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(Menu::Labelling, cx);
            }));
        let card = (self.menu == Some(Menu::Labelling)).then(|| {
            let picked = self.labelling.as_ref().map(|it| it.project.clone());
            let rows = self
                .workspace
                .read(cx)
                .projects
                .iter()
                .map(|open| {
                    let path = open.path.clone();
                    menu::row(
                        Item::action(open.name())
                            .with_icon(icons::files::Folder)
                            .checked(picked.as_ref() == Some(&path)),
                        move |this, _, cx| this.open_labels(path.clone(), cx),
                    )
                })
                .collect();
            let id = SharedString::from("labels-project-menu");
            popover::anchored_menu_below(id.clone(), self.menu_card(id, rows, window, cx), None)
        });
        self.menu_press(trigger, menu, cx)
            .children(card)
            .into_any_element()
    }

    /// One label as it is listed: its chip, what it is for, how many carry
    /// it, and what can be done to it.
    fn label_row(
        &self,
        ix: usize,
        name: String,
        count: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let description = workspace.projects[at]
            .labels
            .get(&name)
            .and_then(|label| label.description.clone())
            .unwrap_or_default();
        let path = workspace.projects[at].path.clone();
        let tint = self.label_tint(Some(at), &name, &theme, cx);
        let carried = match count {
            1 => "1 entry".to_owned(),
            n => format!("{n} entries"),
        };
        div()
            .id(("label-row", ix))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.))
            .py(px(8.))
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(140.))
                    .flex_none()
                    .min_w_0()
                    .child(theme.chip(name.clone(), tint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_style(TextStyle::Callout)
                    .text_color(theme.text_muted)
                    .child(description),
            )
            .child(
                div()
                    .id(("label-count", ix))
                    .flex_none()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .when(count > 0, |el| {
                        el.cursor_pointer().hover(|el| el.text_color(theme.text))
                    })
                    .child(carried)
                    .when(count > 0, |el| {
                        let name = name.clone();
                        el.on_click(cx.listener(move |this, _, window, cx| {
                            this.labelling = None;
                            this.library_on_label(path.clone(), name.clone(), window, cx);
                        }))
                    }),
            )
            .child(
                theme
                    .button("Edit", ButtonStyle::Ghost, None)
                    .id(("label-edit", ix))
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, window, cx| this.edit_label(Some(name.clone()), window, cx)
                    })),
            )
            .child(
                theme
                    .button("Delete", ButtonStyle::Ghost, None)
                    .id(("label-delete", ix))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let count = this
                            .workspace
                            .read(cx)
                            .label_counts()
                            .get(&name)
                            .copied()
                            .unwrap_or_default();
                        this.ask_delete_label(name.clone(), count, cx);
                    })),
            )
            .into_any_element()
    }

    /// The open form: a new label's, or one row's in that row's place.
    fn label_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let Some(form) = self.labelling.as_ref().and_then(|it| it.form.as_ref()) else {
            return div().into_any_element();
        };
        let typed = form.name.read(cx).content().to_string();
        let preview = artifact::label::normalize(&typed).unwrap_or_else(|| "label".to_owned());
        let tint = match form.color {
            Some(paint) => paint.solid(&theme),
            None => multi_select::tint(&theme, &preview),
        };
        // The well opens the card the settings' colour rows open, with Auto
        // in their Default's place: the colour read off the name.
        let well = theme
            .color_well(tint)
            .id("label-color")
            .relative()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_menu(Menu::LabelColor, cx);
            }));
        let card = (self.menu == Some(Menu::LabelColor)).then(|| {
            let card = color::card(
                &theme,
                "label-color",
                presets(),
                form.color,
                Some(form.picker.clone()),
                Some("Auto"),
                |this: &mut Self, paint, _, cx| this.set_form_color(paint, cx),
                cx,
            )
            // Pressing away puts it away, and the press is spent on that.
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.menu == Some(Menu::LabelColor) {
                    this.menu = None;
                    cx.notify();
                }
                cx.stop_propagation();
            }));
            popover::anchored_menu_below("label-color-card", card.into_any_element(), None)
        });
        let well = self.menu_press(well, Menu::LabelColor, cx).children(card);
        div()
            .id("label-form")
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(Theme::control_radius()))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_raised)
            .child(theme.chip(preview, tint))
            .child(self.form_row("Name", form.name.clone().into_any_element(), cx))
            .child(self.form_row(
                "Description",
                form.description.clone().into_any_element(),
                cx,
            ))
            .child(self.form_row("Color", well.into_any_element(), cx))
            .children(form.error.clone().map(|why| {
                div()
                    .text_style(TextStyle::Caption)
                    .text_color(theme.danger)
                    .child(why)
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        theme
                            .button("Cancel", ButtonStyle::Ghost, None)
                            .id("label-cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_label(&DismissLabel, window, cx)
                            })),
                    )
                    .child(
                        theme
                            .button("Save", ButtonStyle::Prominent, None)
                            .id("label-save")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit_label(&CommitLabel, window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    fn form_row(&self, label: &str, value: AnyElement, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .w(px(LABEL_WIDTH))
                    .pt(px(4.))
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(label.to_owned()),
            )
            .child(div().flex_1().min_w_0().child(value))
            .into_any_element()
    }
}
