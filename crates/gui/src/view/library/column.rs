//! Labels in the library: the column, the filter on its heading, and the
//! selection's Label button — each opens [`crate::view::kit::labels`]'s picker.

use super::ROW_GROUP;
use crate::view::{kit::labels::Target, sidebar::Row, window::root::Cydonia};
use bezel::{
    gpui::{AnyElement, Context, Div, SharedString, Stateful, div, prelude::*, px},
    theme::Theme,
    ui::{
        table,
        widgets::{ButtonStyle, Buttons, Content},
    },
};

impl Cydonia {
    /// A listing's labels, as one line of chips cut at the cell's edge. Empty
    /// shows nothing until the row is hovered. Opens the picker on the entry.
    pub(super) fn label_cell(
        &self,
        row: &Row,
        labels: Option<&[String]>,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(labels) = labels else {
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
            .overflow_hidden();
        self.label_chips(cell, target, labels, ROW_GROUP, cx.entity().downgrade(), cx)
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
            .id("library-heading-labels")
            .relative()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.toggle_labels(Target::Heading, window, cx);
            }))
            .children(self.label_picker(&Target::Heading));
        Self::labels_trigger(cell, Target::Heading, cx.entity().downgrade()).into_any_element()
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
        Self::labels_trigger(button, Target::Selection, cx.entity().downgrade())
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
        let agent = library
            .agent
            .as_ref()
            .filter(|_| library.tab.fields().contains(&super::Field::Agent))
            .map(|name| SharedString::from(format!("Agent: {name}")));
        let labels = (library.tab.labelled() && !library.labels.is_empty())
            .then(|| SharedString::from(format!("Labels: {}", library.labels.join(", "))));
        if project.is_none() && agent.is_none() && labels.is_none() {
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
                .children(agent.map(|label| {
                    theme
                        .tag(label)
                        .id("library-filter-agent")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.library_agent(None, cx)))
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
}

/// Whether `held` passes the label filter `wanted`: any one of them will do,
/// and no filter passes everything.
pub(super) fn passes(held: Option<&[String]>, wanted: &[String]) -> bool {
    wanted.is_empty() || held.is_some_and(|held| held.iter().any(|label| wanted.contains(label)))
}
