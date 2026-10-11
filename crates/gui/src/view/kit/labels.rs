//! The label picker: one entry's labels, the library's filter, or its
//! selection's, opened from whichever trigger names it.

use crate::view::{sidebar::Row, window::root::Cydonia};
use bezel::{
    gpui::{
        AnyElement, App, Context, Div, Entity, InteractiveElement, Stateful, Subscription,
        WeakEntity, Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        multi_select::{self, Check, Choice, MultiSelect, MultiSelectEvent},
        popover,
        widgets::Content as _,
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
    /// Open the label picker on `target`, or shut it when the press that led
    /// here found it open — see [`Self::labels_trigger`].
    pub(crate) fn toggle_labels(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.take_press_was_open() {
            popover::close_popup(self, cx, |this: &mut Self| &mut this.picker);
            return;
        }
        self.close_menu(cx);
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
        let events = cx.subscribe_in(
            &select,
            window,
            |this, _, event: &MultiSelectEvent, window, cx| this.label_event(event, window, cx),
        );
        self.picker.open(Picker {
            target,
            select,
            _events: events,
        });
        cx.notify();
    }

    /// `el`, noting each press for `target`'s picker, so a press on the
    /// trigger of the picker that is open shuts it rather than reopening it.
    /// Through `root`, so a view other than the window can hang a trigger.
    pub(crate) fn labels_trigger<E: InteractiveElement>(
        el: E,
        target: Target,
        root: WeakEntity<Cydonia>,
    ) -> E {
        // Capture, as [`popover::trigger_press_matching`] does, so the note
        // lands before any dismissal in the frame reads it.
        el.capture_any_mouse_down(move |_, _, cx| {
            let target = target.clone();
            root.update(cx, |root, _| {
                root.picker
                    .note_trigger_press_matching(|picker| picker.target == target)
            })
            .ok();
        })
    }

    /// `labels` as chips in `trigger`, which a press opens the picker from.
    /// With none, a faint "Add label" shows while `group` is hovered.
    pub(crate) fn label_chips(
        &self,
        trigger: Stateful<Div>,
        target: Target,
        labels: &[String],
        group: &'static str,
        root: WeakEntity<Cydonia>,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = Theme::of(cx).clone();
        let chips = trigger
            .cursor_pointer()
            .on_click({
                let (target, root) = (target.clone(), root.clone());
                move |_, window, cx| {
                    cx.stop_propagation();
                    root.update(cx, |root, cx| {
                        root.toggle_labels(target.clone(), window, cx)
                    })
                    .ok();
                }
            })
            .children(
                labels
                    .iter()
                    .map(|label| theme.chip(label.clone(), multi_select::tint(&theme, label))),
            )
            .when(labels.is_empty(), |chips| {
                chips.child(
                    div()
                        .invisible()
                        .group_hover(group, |el| el.visible())
                        .text_style(TextStyle::Callout)
                        .text_color(theme.text_faint)
                        .child("Add label"),
                )
            })
            .children(self.label_picker(&target));
        Self::labels_trigger(chips, target, root)
    }

    /// The picker, anchored under its trigger, while `target`'s is open.
    pub(crate) fn label_picker(&self, target: &Target) -> Option<AnyElement> {
        let picker = self.picker.get()?;
        (picker.target == *target).then(|| {
            popover::anchored_menu_below(
                "library-labels",
                picker.select.clone().into_any_element(),
                self.picker.closing_since(),
            )
        })
    }

    fn label_event(
        &mut self,
        event: &MultiSelectEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.picker.as_open().map(|picker| picker.target.clone()) else {
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
                    .label_counts(cx)
                    .get(name.as_ref())
                    .copied()
                    .unwrap_or_default();
                self.ask_delete_label(name.to_string(), count, window, cx);
                return;
            }
            MultiSelectEvent::Dismissed => {
                popover::close_popup(self, cx, |this: &mut Self| &mut this.picker);
                return;
            }
        }
        let choices = self.label_choices(&target, cx);
        if let Some(picker) = self.picker.as_open() {
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
                workspace.swap_label(at, &showing, name, on.then(|| name.to_owned()), cx);
            }
        });
    }

    /// Every label in every open project, as `target` sees it.
    fn label_choices(&self, target: &Target, cx: &App) -> Vec<Choice> {
        let workspace = self.workspace.read(cx);
        let counts = workspace.label_counts(cx);
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
            workspace
                .labels_of(at, &showing, cx)
                .map(<[String]>::to_vec)
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
