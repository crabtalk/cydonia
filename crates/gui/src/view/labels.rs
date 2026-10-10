//! The label picker: one entry's labels, the library's filter, or its
//! selection's, opened from whichever trigger names it.

use crate::view::{component::menu::Menu, root::Cydonia, sidebar::Row};
use bezel::{
    gpui::{AnyElement, App, Context, Entity, Pixels, Point, Subscription, Window, prelude::*, px},
    ui::{
        multi_select::{Check, Choice, MultiSelect, MultiSelectEvent},
        popover,
    },
};

/// What an open label picker acts on.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// One entry's labels, from its library cell.
    Cell(Row),
    /// An article's labels, from the line under its title.
    Page(Row),
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
    /// Open the label picker on `target`, or shut it. `at` is where it is to
    /// stand, as [`Cydonia::toggle_menu_at`] takes it.
    pub(crate) fn toggle_labels(
        &mut self,
        target: Target,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let menu = Menu::Labels(target.clone());
        self.toggle_menu_at(menu.clone(), at, cx);
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
        self.picker = Some(Picker {
            target,
            select,
            _events: events,
        });
    }

    /// The picker, anchored under its trigger, while `target`'s is open.
    pub(crate) fn label_picker(&self, target: &Target) -> Option<AnyElement> {
        if self.menu.as_ref() != Some(&Menu::Labels(target.clone())) {
            return None;
        }
        let picker = self.picker.as_ref()?;
        (picker.target == *target).then(|| {
            popover::anchored_menu_below(
                "library-labels",
                picker.select.clone().into_any_element(),
                None,
            )
        })
    }

    fn label_event(&mut self, event: &MultiSelectEvent, cx: &mut Context<Self>) {
        let Some(target) = self.picker.as_ref().map(|picker| picker.target.clone()) else {
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
                if matches!(self.menu, Some(Menu::Labels(_))) {
                    self.menu = None;
                }
                cx.notify();
                return;
            }
        }
        let choices = self.label_choices(&target, cx);
        if let Some(picker) = &self.picker {
            picker
                .select
                .update(cx, |select, cx| select.set_choices(choices, cx));
        }
        cx.notify();
    }

    /// Put `name` on what `target` names, or take it off.
    fn apply_label(&mut self, target: &Target, name: &str, on: bool, cx: &mut Context<Self>) {
        let rows = match target {
            Target::Cell(row) | Target::Page(row) => vec![row.clone()],
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
            Target::Cell(row) | Target::Page(row) => vec![row.clone()],
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
}
