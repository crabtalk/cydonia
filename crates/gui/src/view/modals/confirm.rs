//! The question asked before a delete. Raised from the pane header's `···` and
//! from a card's, so it lives beside neither.

use crate::{
    model::workspace::Showing,
    view::{
        kit::dialog::{answers, dialog},
        sidebar::{Group, Row},
        window::root::Cydonia,
    },
};
use bezel::{
    gpui::{
        App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, WeakEntity, Window, div,
        prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::popover,
};

/// What a pending delete is aimed at.
#[derive(Clone)]
pub(crate) enum Doomed {
    /// A board, session, article or table, by the row the header's `···` acts
    /// on.
    Entry(Row),
    /// One card, by the board it is on and its own id.
    Card(String, String),
    /// One column, by the board it is on and its own id.
    Column(String, String),
    /// A session's last turn, by the session and the item its message is.
    Turn(u64, usize),
    /// Several at once — what the library's selection deletes.
    Many(Vec<Doomed>),
    /// A label, off every entry in every open project carrying it.
    Label(String),
}

/// A delete that has been asked for and not yet agreed to, as a modal.
pub(crate) struct Confirm {
    doomed: Doomed,
    /// What to call it. Taken when the question is asked, not when it draws.
    label: String,
    /// Where it lives, on its own line in the mono face. Nothing for what has
    /// no place on disk to point at.
    goes: Option<String>,
    /// The sentence under it.
    note: String,
    root: WeakEntity<Cydonia>,
    focus: FocusHandle,
}

impl EventEmitter<DismissEvent> for Confirm {}

impl Focusable for Confirm {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Confirm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let dialog = dialog(
            "delete-dialog",
            &theme,
            window,
            [popover::dialog_title(&theme, format!("Delete {}?", self.label)).into_any_element()]
                .into_iter()
                // Quoted rather than spoken: the mono face says this is a
                // thing on disk, not a turn of phrase.
                .chain(self.goes.clone().map(|goes| {
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
                        .into_any_element()
                }))
                .chain([
                    popover::dialog_body(&theme, self.note.clone()).into_any_element(),
                    answers(
                        &theme,
                        "delete",
                        Some(Box::new(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
                        "Delete",
                        Box::new(cx.listener(|this, _, window, cx| {
                            let doomed = this.doomed.clone();
                            let _ = this
                                .root
                                .update(cx, |root, cx| root.delete_doomed(&doomed, window, cx));
                            cx.emit(DismissEvent);
                        })),
                    )
                    .into_any_element(),
                ]),
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        );
        div().track_focus(&self.focus).child(dialog)
    }
}

impl Cydonia {
    /// Raise the question. Delete is the one thing here that cannot be taken
    /// back — Archive, right above it, is the reversible answer.
    pub(crate) fn ask_delete(&mut self, entry: &Row, window: &mut Window, cx: &mut Context<Self>) {
        let label = entry.label(self.workspace.read(cx), cx);
        let (goes, note) = self.goes_with(entry, cx);
        self.ask(
            Confirm {
                doomed: Doomed::Entry(entry.clone()),
                label,
                goes,
                note,
                root: cx.entity().downgrade(),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        );
    }

    /// The same for several entries and cards at once.
    pub(crate) fn ask_delete_many(
        &mut self,
        doomed: Vec<Doomed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if doomed.is_empty() {
            return;
        }
        let label = match doomed.len() {
            1 => "1 item".to_owned(),
            n => format!("{n} items"),
        };
        self.ask(
            Confirm {
                doomed: Doomed::Many(doomed),
                label,
                goes: None,
                note: "Every file goes with them. This cannot be undone.".to_owned(),
                root: cx.entity().downgrade(),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        );
    }

    /// Put `confirm` up, closing any menu it was asked from.
    fn ask(&mut self, confirm: Confirm, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        self.toggle_modal(window, cx, |_, _| confirm);
    }

    /// Carry out a delete that has been agreed to.
    fn delete_doomed(&mut self, doomed: &Doomed, window: &mut Window, cx: &mut Context<Self>) {
        match doomed {
            Doomed::Entry(entry) => self.delete_entry(entry, window, cx),
            Doomed::Card(board, card) => self.delete_card(board, card, cx),
            Doomed::Column(board, column) => self.drop_column(board, column, cx),
            Doomed::Turn(id, at) => self.rewind(*id, *at, window, cx),
            Doomed::Label(name) => {
                self.workspace
                    .update(cx, |workspace, cx| workspace.delete_label(name, cx));
                if let Some(library) = &self.library {
                    library.update(cx, |library, cx| {
                        library.drop_label_filter(name);
                        cx.notify();
                    });
                }
            }
            Doomed::Many(all) => {
                for doomed in all {
                    self.delete_doomed(doomed, window, cx);
                }
            }
        }
    }

    /// The same for a label, carried by `count` entries.
    pub(crate) fn ask_delete_label(
        &mut self,
        name: String,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ask(
            Confirm {
                label: format!("the label \u{201c}{name}\u{201d}"),
                doomed: Doomed::Label(name),
                goes: None,
                note: match count {
                    1 => "It comes off the 1 entry carrying it. This cannot be undone.".to_owned(),
                    n => {
                        format!("It comes off the {n} entries carrying it. This cannot be undone.")
                    }
                },
                root: cx.entity().downgrade(),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        );
    }

    /// The same for one card, which has no file of its own to quote — it lives
    /// inside the board's.
    pub(crate) fn ask_delete_card(
        &mut self,
        on: &str,
        card: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = self
            .workspace
            .read(cx)
            .board_at(on, cx)
            .and_then(|board| Some((board, board.card(card)?)))
            .map(|(board, found)| board.handle_of(found).unwrap_or_else(|| found.text.clone()))
            .unwrap_or_default();
        self.ask(
            Confirm {
                doomed: Doomed::Card(on.to_owned(), card.to_owned()),
                label,
                goes: None,
                note: "This cannot be undone.".to_owned(),
                root: cx.entity().downgrade(),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        );
    }

    /// The same for one lane. A lane is only droppable while
    /// it is empty — see [`artifact::board::Board::remove_column`] — so nothing
    /// in here speaks for the cards.
    pub(crate) fn ask_delete_column(
        &mut self,
        on: &str,
        column: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = self
            .workspace
            .read(cx)
            .board_at(on, cx)
            .and_then(|board| board.column(column))
            .map(|column| column.name.clone())
            .unwrap_or_default();
        self.ask(
            Confirm {
                doomed: Doomed::Column(on.to_owned(), column.to_owned()),
                label,
                goes: None,
                note: "This cannot be undone.".to_owned(),
                root: cx.entity().downgrade(),
                focus: cx.focus_handle(),
            },
            window,
            cx,
        );
    }

    /// Take a session's last message back into its composer. Asked first when
    /// a reply goes with it.
    pub(crate) fn ask_rewind(
        &mut self,
        id: u64,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answered = self
            .workspace
            .read(cx)
            .session(id, cx)
            .is_some_and(|chat| chat.items.len() > at + 1);
        if !answered {
            self.rewind(id, at, window, cx);
            return;
        }
        self.ask(Confirm {
            doomed: Doomed::Turn(id, at),
            label: "the last turn".to_owned(),
            goes: None,
            note: "The reply goes with it, and the agent restarts without it. Files it changed stay changed."
                .to_owned(),
            root: cx.entity().downgrade(),
            focus: cx.focus_handle(),
        }, window, cx);
    }

    fn rewind(&mut self, id: u64, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self
            .workspace
            .update(cx, |workspace, cx| workspace.rewind_session(id, at, cx))
        else {
            return;
        };
        let composers: Vec<_> = self
            .leaves
            .iter()
            .map(|pane| pane.read(cx).composer.clone())
            .chain(self.session_cards.get(&id).cloned())
            .filter(|composer| composer.read(cx).session() == Some(id))
            .collect();
        let Some(first) = composers.first() else {
            self.workspace
                .update(cx, |workspace, cx| workspace.set_draft(id, text, cx));
            return;
        };
        window.focus(&first.focus_handle(cx), cx);
        for composer in composers {
            composer.update(cx, |composer, cx| composer.restore(text.clone(), cx));
        }
    }

    /// Where this entry lives and what to say about losing it: a path under
    /// `.cydonia/`, or for a table the database it is dropped out of.
    fn goes_with(&self, entry: &Row, cx: &App) -> (Option<String>, String) {
        const UNDONE: &str = "This cannot be undone.";
        let workspace = self.workspace.read(cx);
        let at = |path: Option<String>| (path, UNDONE.to_owned());
        let located = entry.located(self.workspace.read(cx));
        let open = located.and_then(|(project, _)| workspace.projects.get(project));
        let ix = located.map_or(0, |(_, ix)| ix);
        match entry {
            // A space holds none of what it arranges, so nothing but the
            // arrangement itself goes.
            Row::Group(Group::Space(id)) => workspace
                .space_ix(id)
                .map(|ix| &workspace.spaces[ix])
                .map(|space| {
                    (
                        // Beside the config, not in a project: a space spans
                        // them — see [`crate::model::spaces`].
                        Some(format!("~/.config/cydonia/spaces/{}.toml", space.id)),
                        format!("The entries it arranges stay where they are. {UNDONE}"),
                    )
                })
                .unwrap_or_else(|| at(None)),
            Row::Entry {
                showing: Showing::Session(id),
                ..
            } => open
                .and_then(|open| open.session(*id, cx))
                .and_then(|chat| chat.filed())
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
            Row::Entry {
                showing: Showing::Board(_),
                ..
            } => at(open
                .and_then(|open| open.boards.get(ix))
                .map(|board| format!(".cydonia/boards/{}/", board.id))),
            Row::Entry {
                showing: Showing::Article(_),
                ..
            } => at(open
                .and_then(|open| open.articles.get(ix))
                .and_then(|article| article.path.parent())
                .and_then(|dir| dir.file_name())
                .map(|dir| format!(".cydonia/articles/{}/", dir.to_string_lossy()))),
            // Dropped out of the database rather than unlinked: the file is
            // where to look, not what goes.
            Row::Entry {
                showing: Showing::Table(_),
                ..
            } => {
                let table = open.and_then(|open| open.tables.get(ix));
                let rows = table.map_or(0, |table| table.rows);
                (
                    table.map(|table| format!("{} in .cydonia/{}", table.key, crate::data::FILE)),
                    format!(
                        "Its {rows} rows are dropped; other tables and the database stay. {UNDONE}"
                    ),
                )
            }
            Row::Group(Group::Project(_)) | Row::Archive(_) | Row::Heading(_) => {
                (None, UNDONE.to_owned())
            }
        }
    }
}
