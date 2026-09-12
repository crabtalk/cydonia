//! The question asked before a delete. Raised from the pane header's `···` and
//! from a card's, so it lives beside neither.

use crate::view::{root::Cydonia, sidebar::Row};
use bezel::{
    gpui::{AnyElement, App, Context, div, prelude::*, px},
    theme::{TextStyle, Theme, Typeset},
    ui::widgets::{ButtonStyle, Buttons, Scaffolding as _},
};

/// What a pending delete is aimed at.
#[derive(Clone)]
pub(crate) enum Doomed {
    /// A board, session, article or table, by the row the header's `···` acts
    /// on.
    Entry(Row),
    /// One card on the open board, by id.
    Card(String),
}

/// A delete that has been asked for and not yet agreed to.
pub(crate) struct Confirming {
    pub doomed: Doomed,
    /// What to call it. Taken when the question is asked: a dialog that
    /// changed its mind mid-question would be worse than a stale name.
    pub label: String,
    /// Where it lives, on its own line in the mono face — a path is read
    /// character by character, and forty of them wrap badly mid-clause.
    /// Nothing for what has no place on disk to point at.
    pub goes: Option<String>,
    /// The sentence under it.
    pub note: String,
}

impl Cydonia {
    /// Raise the question. Delete is the one thing here that cannot be taken
    /// back — Archive, right above it, is the reversible answer.
    pub(crate) fn ask_delete(&mut self, entry: Row, cx: &mut Context<Self>) {
        let label = self
            .showing(cx)
            .and_then(|pane| self.toolbar(pane, cx))
            .map(|toolbar| toolbar.title)
            .unwrap_or_default();
        let (goes, note) = self.goes_with(entry, cx);
        self.menu = None;
        self.confirming = Some(Confirming {
            doomed: Doomed::Entry(entry),
            label,
            goes,
            note,
        });
        cx.notify();
    }

    /// The same for one card, which has no file of its own to quote — it lives
    /// inside the board's.
    pub(crate) fn ask_delete_card(&mut self, card: &str, cx: &mut Context<Self>) {
        let label = self
            .workspace
            .read(cx)
            .active_board()
            .and_then(|board| Some((board, board.card(card)?)))
            .map(|(board, found)| board.handle_of(found).unwrap_or_else(|| found.text.clone()))
            .unwrap_or_default();
        self.menu = None;
        self.confirming = Some(Confirming {
            doomed: Doomed::Card(card.to_owned()),
            label,
            goes: None,
            note: "This cannot be undone.".to_owned(),
        });
        cx.notify();
    }

    /// Where this entry lives and what to say about losing it: a path under
    /// `.cydonia/`, or for a table the database it is dropped out of.
    fn goes_with(&self, entry: Row, cx: &App) -> (Option<String>, String) {
        const UNDONE: &str = "This cannot be undone.";
        let workspace = self.workspace.read(cx);
        let at = |path: Option<String>| (path, UNDONE.to_owned());
        match entry {
            Row::Session { project, id } => workspace
                .projects
                .get(project)
                .and_then(|open| open.session(id))
                .and_then(|chat| chat.record.as_deref())
                .map(|record| at(Some(format!(".cydonia/sessions/{record}.json"))))
                // A session is filed from its first turn — `flush` leaves
                // early while there are no items. So this one has had none, not
                // a kind that goes unwritten.
                .unwrap_or_else(|| {
                    (
                        None,
                        format!("It has had no turn, so nothing on disk goes with it. {UNDONE}"),
                    )
                }),
            Row::Board { project, ix } => at(workspace
                .projects
                .get(project)
                .and_then(|open| open.boards.get(ix))
                .map(|board| format!(".cydonia/boards/{}.toml", board.id))),
            Row::Article { project, ix } => at(workspace
                .projects
                .get(project)
                .and_then(|open| open.articles.get(ix))
                .and_then(|article| article.path.parent())
                .and_then(|dir| dir.file_name())
                .map(|dir| format!(".cydonia/articles/{}/", dir.to_string_lossy()))),
            // Dropped out of the database rather than unlinked: the file is
            // where to look, not what goes.
            Row::Table { project, ix } => {
                let rows = workspace
                    .projects
                    .get(project)
                    .and_then(|open| open.tables.get(ix))
                    .map_or(0, |table| table.rows);
                (
                    Some(format!(".cydonia/{}", crate::data::FILE)),
                    format!("Its {rows} rows are dropped; the database stays. {UNDONE}"),
                )
            }
            Row::Project(_) | Row::Archive(_) => (None, UNDONE.to_owned()),
        }
    }

    /// The question, over a scrim that takes the press dismissing it — the
    /// same shape as the settings window's dialog.
    pub(crate) fn confirm_delete(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirming = self.confirming.as_ref()?;
        let theme = Theme::of(cx).clone();
        let doomed = confirming.doomed.clone();
        Some(
            div()
                .id("delete-scrim")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.scrim())
                .on_click(cx.listener(|this, _, _, cx| this.dismiss_delete(cx)))
                .child(
                    div()
                        .id("delete-dialog")
                        .w(px(360.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(20.))
                        .rounded(px(Theme::panel_radius()))
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        // The press that answers must not reach the scrim.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(theme.row_title(format!("Delete {}?", confirming.label)))
                        // Quoted rather than spoken: the mono face says this
                        // is a thing on disk, not a turn of phrase.
                        .children(confirming.goes.clone().map(|goes| {
                            div()
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(Theme::control_radius()))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface_raised)
                                .text_style(TextStyle::Subheadline)
                                .font_family(theme.font_mono.clone())
                                .text_color(theme.text_muted)
                                .child(goes)
                        }))
                        .child(
                            div()
                                .text_style(TextStyle::Subheadline)
                                .text_color(theme.text_muted)
                                .child(confirming.note.clone()),
                        )
                        .child(
                            div()
                                .mt(px(4.))
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    theme
                                        .button("Cancel", ButtonStyle::Ghost, None)
                                        .id("delete-cancel")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.dismiss_delete(cx)),
                                        ),
                                )
                                .child(
                                    theme
                                        .button("Delete", ButtonStyle::Prominent, None)
                                        .id("delete-confirm")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.confirming = None;
                                            match &doomed {
                                                Doomed::Entry(entry) => {
                                                    this.delete_entry(*entry, cx)
                                                }
                                                Doomed::Card(card) => this.delete_card(card, cx),
                                            }
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn dismiss_delete(&mut self, cx: &mut Context<Self>) {
        self.confirming = None;
        cx.notify();
    }
}
