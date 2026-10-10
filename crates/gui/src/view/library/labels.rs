//! Labels in the library: the column, the filter on its heading, the
//! selection's Label button, and the one picker all three open.

use super::{Item, ROW_GROUP};
use crate::view::{component::menu::Menu, root::Cydonia, sidebar::Row};
use bezel::{
    gpui::{
        AnyElement, App, Context, Div, Entity, SharedString, Stateful, Subscription, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        multi_select::{self, Check, Choice, MultiSelect, MultiSelectEvent},
        popover, table,
        widgets::{ButtonStyle, Buttons, Content},
    },
};

/// What an open label picker acts on.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// One entry's labels, from its cell.
    Cell(Row),
    /// The library's label filter, from the column heading.
    Heading,
    /// Every entry in the selection, from the selection bar.
    Selection,
}

/// The open label picker.
pub(crate) struct Picker {
    target: Target,
    select: Entity<MultiSelect>,
    _events: Subscription,
}

impl Cydonia {
    /// Open the label picker on `target`, or shut it.
    fn toggle_labels(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let menu = Menu::LibraryLabels(target.clone());
        self.toggle_menu(menu.clone(), cx);
        if self.menu != Some(menu) {
            return;
        }
        let choices = self.label_choices(&target, cx);
        let creates = target != Target::Heading;
        let select = cx.new(|cx| {
            let select = MultiSelect::new(choices, cx)
                .with_manage()
                .with_width(px(200.), px(280.));
            match creates {
                true => select.with_create(|text| artifact::label::normalize(text).map(Into::into)),
                false => select,
            }
        });
        select.update(cx, |select, cx| select.reset(window, cx));
        let events = cx.subscribe(&select, |this, _, event: &MultiSelectEvent, cx| {
            this.label_event(event, cx)
        });
        if let Some(library) = &mut self.library {
            library.picker = Some(Picker {
                target,
                select,
                _events: events,
            });
        }
    }

    /// The picker, anchored under its trigger, while `target`'s is open.
    fn label_picker(&self, target: &Target) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&Menu::LibraryLabels(target.clone())) {
            return None;
        }
        let picker = self.library.as_ref()?.picker.as_ref()?;
        (picker.target == *target).then(|| {
            popover::anchored_menu_below(
                "library-labels",
                picker.select.clone().into_any_element(),
                None,
            )
        })
    }

    fn label_event(&mut self, event: &MultiSelectEvent, cx: &mut Context<Self>) {
        let Some(target) = self
            .library
            .as_ref()
            .and_then(|library| library.picker.as_ref())
            .map(|picker| picker.target.clone())
        else {
            return;
        };
        match event {
            MultiSelectEvent::Toggled { name, on } => self.apply_label(&target, name, *on, cx),
            MultiSelectEvent::Created(name) => self.apply_label(&target, name, true, cx),
            MultiSelectEvent::Renamed { from, to } => {
                self.workspace
                    .update(cx, |workspace, cx| workspace.rename_label(from, to, cx));
                if let Some(library) = &mut self.library {
                    for label in &mut library.labels {
                        if label == from.as_ref() {
                            *label = to.to_string();
                        }
                    }
                    library.labels = artifact::label::normalize_all(&library.labels);
                }
            }
            MultiSelectEvent::Deleted(name) => {
                let count = self
                    .workspace
                    .read(cx)
                    .label_counts()
                    .get(name.as_ref())
                    .copied()
                    .unwrap_or_default();
                self.ask_delete_label(name.to_string(), count, cx);
                return;
            }
            MultiSelectEvent::Dismissed => {
                if matches!(self.menu, Some(Menu::LibraryLabels(_))) {
                    self.menu = None;
                }
                cx.notify();
                return;
            }
        }
        let choices = self.label_choices(&target, cx);
        if let Some(picker) = self
            .library
            .as_ref()
            .and_then(|library| library.picker.as_ref())
        {
            picker
                .select
                .update(cx, |select, cx| select.set_choices(choices, cx));
        }
        cx.notify();
    }

    /// Put `name` on what `target` names, or take it off.
    fn apply_label(&mut self, target: &Target, name: &str, on: bool, cx: &mut Context<Self>) {
        let rows = match target {
            Target::Cell(row) => vec![row.clone()],
            Target::Selection => self.selected_rows(),
            Target::Heading => {
                if let Some(library) = &mut self.library {
                    library.labels.retain(|label| label != name);
                    if on {
                        library.labels.push(name.to_owned());
                    }
                    library.labels.sort();
                }
                return;
            }
        };
        self.workspace.update(cx, |workspace, cx| {
            for row in rows {
                let Row::Entry { project, showing } = row else {
                    continue;
                };
                let Some(at) = workspace.project_at(&project) else {
                    continue;
                };
                let Some(held) = workspace.labels_of(at, &showing) else {
                    continue;
                };
                let mut labels: Vec<String> = held
                    .iter()
                    .filter(|label| *label != name)
                    .cloned()
                    .collect();
                if on {
                    labels.push(name.to_owned());
                }
                workspace.set_labels(at, &showing, labels, cx);
            }
        });
    }

    /// Every label in every open project, as `target` sees it.
    fn label_choices(&self, target: &Target, cx: &App) -> Vec<Choice> {
        let workspace = self.workspace.read(cx);
        let counts = workspace.label_counts();
        let held: Vec<Vec<String>> = match target {
            Target::Cell(row) => vec![row.clone()],
            Target::Selection => self.selected_rows(),
            Target::Heading => Vec::new(),
        }
        .into_iter()
        .filter_map(|row| {
            let Row::Entry { project, showing } = row else {
                return None;
            };
            let at = workspace.project_at(&project)?;
            workspace.labels_of(at, &showing).map(<[String]>::to_vec)
        })
        .collect();
        let filter = self
            .library
            .as_ref()
            .map(|library| library.labels.clone())
            .unwrap_or_default();
        let mut names: Vec<String> = counts.keys().cloned().collect();
        if *target == Target::Heading {
            names.extend(filter.iter().cloned());
            names.sort();
            names.dedup();
        }
        names
            .into_iter()
            .map(|name| {
                let check = match target {
                    Target::Heading => match filter.contains(&name) {
                        true => Check::On,
                        false => Check::Off,
                    },
                    _ => match held.iter().filter(|labels| labels.contains(&name)).count() {
                        0 => Check::Off,
                        n if n == held.len() => Check::On,
                        _ => Check::Mixed,
                    },
                };
                Choice {
                    count: (*target == Target::Heading)
                        .then(|| counts.get(&name).copied().unwrap_or_default()),
                    name: name.into(),
                    check,
                }
            })
            .collect()
    }

    /// A listing's labels, as one line of chips cut at the cell's edge. Empty
    /// shows nothing until the row is hovered. Opens the picker on the entry.
    pub(super) fn label_cell(
        &self,
        item: &Item,
        labels: Option<&[String]>,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let (Item::Entry(row), Some(labels)) = (item, labels) else {
            return div().into_any_element();
        };
        let target = Target::Cell(row.clone());
        let cell = div()
            .id(("library-labels", ix))
            .relative()
            .min_w_0()
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .overflow_hidden()
            .cursor_pointer()
            .on_click(cx.listener({
                let target = target.clone();
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.toggle_labels(target.clone(), window, cx);
                }
            }))
            .children(
                labels
                    .iter()
                    .map(|label| theme.chip(label.clone(), multi_select::tint(&theme, label))),
            )
            .when(labels.is_empty(), |cell| {
                cell.child(
                    div()
                        .invisible()
                        .group_hover(ROW_GROUP, |el| el.visible())
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_faint)
                        .child("Add label"),
                )
            })
            .children(self.label_picker(&target));
        self.menu_press(cell, Menu::LibraryLabels(target), cx)
            .into_any_element()
    }

    /// The Labels heading: opens the filter.
    pub(super) fn labels_heading(
        &self,
        column: &table::Column,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let cell = table::header_cell(&theme, column, None)
            .child(
                bezel::ui::icons::icon(bezel::ui::icons::arrows::ChevronDown)
                    .size(px(11.))
                    .text_color(theme.text_muted),
            )
            .id(("library-heading", super::LABELS))
            .relative()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_labels(Target::Heading, window, cx);
            }))
            .children(self.label_picker(&Target::Heading));
        self.menu_press(cell, Menu::LibraryLabels(Target::Heading), cx)
            .into_any_element()
    }

    /// The selection bar's Label button.
    pub(super) fn label_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let button = theme
            .button("Label", ButtonStyle::Ghost, None)
            .id("library-label")
            .relative()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_labels(Target::Selection, window, cx);
            }))
            .children(self.label_picker(&Target::Selection));
        self.menu_press(button, Menu::LibraryLabels(Target::Selection), cx)
    }

    /// The filters on, each a pill that takes it off. Nothing while none is.
    pub(super) fn filter_strip(&self, cx: &mut Context<Self>) -> Option<Div> {
        let theme = Theme::of(cx).clone();
        let library = self.library.as_ref()?;
        let workspace = self.workspace.read(cx);
        let project = library
            .project
            .as_ref()
            .and_then(|path| workspace.project_at(path))
            .map(|at| SharedString::from(format!("Project: {}", workspace.projects[at].name())));
        let labels = (!library.labels.is_empty())
            .then(|| SharedString::from(format!("Labels: {}", library.labels.join(", "))));
        if project.is_none() && labels.is_none() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .children(project.map(|label| {
                    theme
                        .tag(label)
                        .id("library-filter-project")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.library_project(None, cx)))
                }))
                .children(labels.map(|label| {
                    theme
                        .tag(label)
                        .id("library-filter-labels")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(library) = &mut this.library {
                                library.labels.clear();
                            }
                            cx.notify();
                        }))
                })),
        )
    }

    /// Whether any entry in the selection can carry labels.
    pub(super) fn selection_labelled(&self, cx: &App) -> bool {
        let workspace = self.workspace.read(cx);
        self.selected_rows().iter().any(|row| match row {
            Row::Entry { project, showing } => workspace
                .project_at(project)
                .is_some_and(|at| workspace.labels_of(at, showing).is_some()),
            _ => false,
        })
    }
}

/// Whether `held` passes the label filter `wanted`: any one of them will do,
/// and no filter passes everything.
pub(super) fn passes(held: Option<&[String]>, wanted: &[String]) -> bool {
    wanted.is_empty() || held.is_some_and(|held| held.iter().any(|label| wanted.contains(label)))
}
