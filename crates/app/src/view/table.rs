//! The table pane: one table's rows, and the rail row that opens it.
//!
//! A reader, deliberately. The agent is what fills a table in — this shows what
//! is in one, and the shape it has.

use crate::{
    data::ColType,
    view::root::{Cydonia, Pane},
};
use bezel::{
    gpui::{AnyElement, Context, SharedString, div, prelude::*, px},
    theme::Theme,
    ui::{icons, table},
};
use serde_json::Value;

impl Cydonia {
    // ── mutations ────────────────────────────────────────────────

    pub(crate) fn new_table(&mut self, project: usize, cx: &mut Context<Self>) {
        self.select_project(project, cx);
        let ix = self
            .workspace
            .update(cx, |workspace, cx| workspace.new_table(cx));
        if let Some(ix) = ix {
            self.open_table(project, ix, cx);
        }
    }

    pub(crate) fn open_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.commit(cx);
        self.workspace
            .update(cx, |workspace, cx| workspace.open_table(project, ix, cx));
        self.pane = Pane::Table;
        cx.notify();
    }

    fn delete_table(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.delete_table(project, ix, cx);
        });
        cx.notify();
    }

    // ── chrome ───────────────────────────────────────────────────

    /// The table. Same frame as [`Cydonia::article`]: the body of the content
    /// card, with the composer stack still pinned under it.
    pub(crate) fn table(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = Theme::of(cx).clone();
        let page = self.workspace.read(cx).active_project()?.page.as_ref()?;
        let columns: Vec<table::Column> = page
            .columns
            .iter()
            .map(|column| {
                let declared = table::Column::new(column.name.clone(), table::Width::Flex(1.));
                match column.kind {
                    ColType::Number => declared.align_end(),
                    _ => declared,
                }
            })
            .collect();

        let head = div()
            .flex_none()
            .px(px(24.))
            .pt(px(20.))
            .pb(px(12.))
            .flex()
            .flex_row()
            .items_baseline()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(15.))
                    .text_color(theme.text)
                    .child(page.name.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme.text_faint)
                    // The table's own count, not the window's — a pane that
                    // stops at its limit without saying so reads as the end.
                    .child(match page.total as usize == page.rows.len() {
                        true => format!("{} rows", page.total),
                        false => format!("{} of {} rows", page.rows.len(), page.total),
                    }),
            );

        let body = table::table(&theme)
            .child(
                table::header(&theme).children(
                    columns
                        .iter()
                        .map(|column| table::header_cell(&theme, column, None)),
                ),
            )
            .children(page.rows.iter().enumerate().map(|(n, record)| {
                table::row(
                    &theme,
                    &columns,
                    n == 0,
                    false,
                    record
                        .cells
                        .iter()
                        .map(|value| cell(value, &theme))
                        .collect(),
                )
            }));

        Some(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(head)
                .child(
                    div()
                        .id("table-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(24.))
                        .pb(px(20.))
                        .child(body),
                )
                .into_any_element(),
        )
    }

    /// One table in the rail, under the project that holds it.
    pub(crate) fn table_row(
        &self,
        project: usize,
        ix: usize,
        name: String,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = Theme::of(cx).clone();
        let workspace = self.workspace.read(cx);
        let selected = self.showing(cx) == Pane::Table
            && workspace.active == Some(project)
            && workspace
                .projects
                .get(project)
                .is_some_and(|open| open.table == Some(ix));
        let tone = if selected {
            theme.text
        } else {
            theme.text_muted
        };

        div()
            .id(SharedString::from(format!("table-{project}-{ix}")))
            .group("table-row")
            .ml(px(18.))
            .mr(px(8.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(Theme::control_radius()))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .when(selected, |el| el.bg(theme.glass_hover()))
            .hover(|el| el.bg(theme.glass_hover()))
            .child(
                icons::icon(icons::WIDGET)
                    .size(px(14.))
                    .flex_none()
                    .text_color(tone),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(tone)
                    .child(name),
            )
            .child(
                div()
                    .id(("delete-table", ix))
                    .flex_none()
                    .invisible()
                    .group_hover("table-row", |el| el.visible())
                    .rounded(px(Theme::control_radius()))
                    .p(px(2.))
                    .child(
                        icons::icon(icons::TRASH_BIN_MINIMALISTIC)
                            .size(px(12.))
                            .text_color(theme.text_faint),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.delete_table(project, ix, cx);
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_table(project, ix, cx);
            }))
    }
}

/// One cell. An empty cell is drawn as nothing rather than as `null` — a blank
/// is what absence looks like in a grid.
fn cell(value: &Value, theme: &Theme) -> AnyElement {
    let text = match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    div()
        .truncate()
        .text_color(theme.text)
        .child(text)
        .into_any_element()
}
