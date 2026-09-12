//! The band across the top of the detail column: what the pane is showing, and
//! the `···` that acts on it.
//!
//! Pulled, not pushed. `../desktop` has each screen register its header into a
//! shared slot (`shell/header/paneTitle.svelte.ts`) because SvelteKit builds a
//! route deep inside the shell, where the shell cannot reach it — and it needed
//! a second mechanism, `activePane.ts`, purely to stop a hidden screen from
//! fighting the visible one for that one slot. Here [`Cydonia::detail`] already
//! builds the body itself, so it asks the pane instead of being told. Nothing
//! registers, nothing unregisters, and two panes showing at once would be this
//! function called twice — see `todos/panels.md`.

use crate::view::{
    component::menu::Menu,
    root::{self, Cydonia, Pane},
    sidebar::{Renaming, Row},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, FontWeight, KeyBinding,
        SharedString, Window, actions, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
        popover,
        widgets::{ButtonStyle, Buttons, Scaffolding as _},
    },
};

actions!(cydonia_header, [CommitInfo, DismissInfo]);

/// Claimed on the identity panel's fields, so `enter` files the panel rather
/// than doing whatever `enter` does in every other field.
const INFO_CONTEXT: &str = "CydoniaBoardInfo";

/// How wide the labels run, so the two fields start on one edge.
const LABEL_WIDTH: f32 = 34.;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", CommitInfo, Some(INFO_CONTEXT)),
        KeyBinding::new("escape", DismissInfo, Some(INFO_CONTEXT)),
    ]);
}

/// A board's identity, open for editing: what it is called, and the key every
/// handle on it carries.
///
/// A panel rather than the inline field the other panes rename with, because a
/// board is named by two things and they are worth seeing together — the key is
/// derived from the name, so changing one is usually changing both.
///
/// Buffered: nothing is written until Save, and dismissing discards. Built when
/// it opens, so it is always seeded from what is there now.
pub(crate) struct BoardInfo {
    /// Which board, by id — the panel outlives a re-read of the project.
    pub board: String,
    pub name: Entity<TextField>,
    pub key: Entity<TextField>,
    /// What was wrong with the last attempt to file it. Keeps the panel open,
    /// because a key that is taken is a thing to fix rather than to be told
    /// about after the fact.
    pub error: Option<SharedString>,
}

/// What a pane puts in the band.
pub(crate) struct Toolbar {
    /// What the pane is showing, by the name the sidebar would list it under.
    pub title: String,
    /// The entry that name belongs to. Every pane has one today; the field is
    /// an option because a pane standing in for something not yet made — see
    /// [`Cydonia::launch`] — has a band and nothing to act on.
    pub entry: Option<Entry>,
}

/// The thing the title names: what to put a menu on, whether that menu offers
/// Archive or Unarchive, and how the thing is renamed.
pub(crate) struct Entry {
    pub row: Row,
    pub archived: bool,
    pub naming: Naming,
}

/// Where an entry is renamed, which is one place per kind and never two.
pub(crate) enum Naming {
    /// In the band, in place — a pane named by one field and nothing else. See
    /// [`Cydonia::header_renaming`].
    Inline(Renaming),
    /// In the board's identity panel, by id. A board is named by two things,
    /// its name and its key, and they are derived from each other — so there is
    /// one place that edits both and no inline field anywhere. See
    /// [`Cydonia::toggle_info`].
    Panel(String),
}

/// A delete that has been asked for and not yet agreed to.
pub(crate) struct Confirming {
    pub entry: Row,
    /// What to call it in the question. Taken when the question is asked: the
    /// entry could be renamed from under an open dialog, and a dialog that
    /// changed its mind about what it was asking would be worse than a stale
    /// name.
    pub label: String,
    /// Where it lives, set apart from the prose. A path is read character by
    /// character rather than taken in as a phrase, so it goes in the mono face
    /// on a line of its own — and out of the sentence, where forty characters
    /// of it would wrap through the middle of a clause.
    ///
    /// Nothing for what has no place on disk to point at.
    pub goes: Option<String>,
    /// The sentence under it.
    pub note: String,
}

/// One of the panel's fields, holding what is there now.
fn seed(content: String, placeholder: &'static str, cx: &mut App) -> Entity<TextField> {
    let field = cx.new(|cx| {
        TextField::new(cx)
            .with_key_context(INFO_CONTEXT)
            .with_placeholder(placeholder)
    });
    field.update(cx, |field, cx| field.set_content(content, cx));
    field
}

impl Cydonia {
    /// Raise the question. Delete is the one thing on the band that cannot be
    /// taken back — Archive, directly above it, is the reversible answer — so
    /// it is the one thing that asks first.
    pub(crate) fn ask_delete(&mut self, entry: Row, cx: &mut Context<Self>) {
        let label = self
            .showing(cx)
            .and_then(|pane| self.toolbar(pane, cx))
            .map(|toolbar| toolbar.title)
            .unwrap_or_default();
        let (goes, note) = self.goes_with(entry, cx);
        self.menu = None;
        self.confirming = Some(Confirming {
            entry,
            label,
            goes,
            note,
        });
        cx.notify();
    }

    /// Where this entry lives and what to say about losing it — the path under
    /// `.cydonia/`, or for a table the database it is dropped out of, which is
    /// not the same as a file that goes.
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
                // A session is filed from its first turn: `ChatSession::flush`
                // leaves early while there are no items and mints the record
                // once there are. So this is the session that has not had a
                // turn yet, not a kind that goes unwritten.
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
            // Dropped out of the database rather than unlinked. The file is
            // where to look, but it is not what goes — so the note says which
            // of the two is happening.
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

    /// The question, over a scrim that takes the press that dismisses it —
    /// the same shape as the settings window's own dialog.
    pub(crate) fn confirm_delete(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirming = self.confirming.as_ref()?;
        let theme = Theme::of(cx).clone();
        let entry = confirming.entry;
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
                        // The path, quoted rather than spoken: the mono face
                        // and the frame around it say this is a thing on disk
                        // and not a turn of phrase.
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
                                            this.delete_entry(entry, cx);
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

    /// What the pane showing in `pane` puts in the band.
    ///
    /// Takes the pane rather than reading [`Cydonia::showing`], so a second
    /// panel is a second call and not a second implementation.
    pub(crate) fn toolbar(&self, pane: Pane, cx: &App) -> Option<Toolbar> {
        let workspace = self.workspace.read(cx);
        let project = workspace.active?;
        let open = workspace.projects.get(project)?;
        Some(match pane {
            Pane::Chat => {
                let chat = workspace.active_session()?;
                Toolbar {
                    title: chat.label(),
                    entry: Some(Entry {
                        row: Row::Session {
                            project,
                            id: chat.id,
                        },
                        archived: chat.closed,
                        naming: Naming::Inline(Renaming::Session(chat.id)),
                    }),
                }
            }
            Pane::Board => {
                let ix = open.board?;
                let board = open.boards.get(ix)?;
                Toolbar {
                    title: board.label().to_owned(),
                    entry: Some(Entry {
                        row: Row::Board { project, ix },
                        archived: board.archived,
                        naming: Naming::Panel(board.id.clone()),
                    }),
                }
            }
            Pane::Article => {
                let ix = open.article?;
                let article = open.articles.get(ix)?;
                Toolbar {
                    title: article.label().to_owned(),
                    entry: Some(Entry {
                        row: Row::Article { project, ix },
                        archived: article.archived,
                        naming: Naming::Inline(Renaming::Article(article.path.clone())),
                    }),
                }
            }
            Pane::Table => {
                let ix = open.table?;
                let table = open.tables.get(ix)?;
                Toolbar {
                    title: table.name.clone(),
                    entry: Some(Entry {
                        row: Row::Table { project, ix },
                        archived: table.archived,
                        naming: Naming::Inline(Renaming::Table(table.key.clone())),
                    }),
                }
            }
        })
    }

    // ── the board's identity panel ───────────────────────────────

    /// Show the panel, or put it away when the press that opened this one is
    /// the press that shut it.
    ///
    /// The panel dismisses on a press outside itself, which lands before the
    /// click — so by the time the name's own handler runs the panel already
    /// reads as shut, and opening it again is all a plain toggle could do. What
    /// the press found is noted on the way down instead.
    fn toggle_info(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        let shut_by_this_press = std::mem::take(&mut self.info_pressed);
        if shut_by_this_press || self.info.is_some() {
            self.info = None;
            cx.notify();
            return;
        }
        self.open_info(board, window, cx);
    }

    /// Open the panel on the board the band is showing, seeded from what it is
    /// called now.
    pub(crate) fn open_info(&mut self, board: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
        // One editor at a time, and before the board is looked up rather than
        // after: a rename left in flight would otherwise outlive a board that
        // has gone, and the band would go on drawing a field for it.
        self.menu = None;
        self.renaming = None;
        let Some((name, key)) = self
            .workspace
            .read(cx)
            .board_at(board)
            .map(|board| (board.name.clone(), board.key.clone()))
        else {
            cx.notify();
            return;
        };
        let name = seed(name, "name this board…", cx);
        let key = seed(key, "KEY", cx);
        window.focus(&name.read(cx).focus_handle(cx), cx);
        self.info = Some(BoardInfo {
            board: board.to_owned(),
            name,
            key,
            error: None,
        });
        cx.notify();
    }

    pub(crate) fn commit_info(&mut self, _: &CommitInfo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = self.info.as_ref() else {
            return;
        };
        let board = info.board.clone();
        let name = info.name.read(cx).content().trim().to_owned();
        let key = info.key.read(cx).content().trim().to_owned();
        let filed = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_board(&board, name, &key, cx)
        });
        match filed {
            Ok(()) => self.info = None,
            // Left open, holding what was typed: a key already taken is
            // something to change, not something to be told about after the
            // panel has thrown the rest of the edit away.
            Err(why) => {
                if let Some(info) = self.info.as_mut() {
                    info.error = Some(why.into());
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn dismiss_info(&mut self, _: &DismissInfo, _: &mut Window, cx: &mut Context<Self>) {
        self.info = None;
        cx.notify();
    }

    /// The panel, hung under the name that opens it.
    fn info_panel(&self, board: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let info = self.info.as_ref().filter(|info| info.board == board)?;
        let theme = Theme::of(cx).clone();
        let card = popover::popover_card(&theme)
            .w(px(280.))
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.info_row("Name", info.name.clone(), cx))
            .child(self.info_row("Key", info.key.clone(), cx))
            // Only when there is something wrong. A standing line explaining
            // what a key is for would be a caption on two labelled fields, and
            // any example it gave would name a board that is not this one.
            .children(info.error.clone().map(|why| {
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
                            .id("info-cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_info(&DismissInfo, window, cx)
                            })),
                    )
                    .child(
                        theme
                            .button("Save", ButtonStyle::Prominent, None)
                            .id("info-save")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.commit_info(&CommitInfo, window, cx)
                            })),
                    ),
            )
            // Pressing away is dismissing, and dismissing discards — what was
            // typed is not filed until Save says so.
            .on_mouse_down_out(
                cx.listener(|this, _, window, cx| this.dismiss_info(&DismissInfo, window, cx)),
            )
            .into_any_element();
        Some(popover::anchored_menu_below(
            SharedString::from("board-info"),
            card,
            None,
        ))
    }

    fn info_row(&self, label: &str, field: Entity<TextField>, cx: &Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .w(px(LABEL_WIDTH))
                    .text_style(TextStyle::Caption)
                    .text_color(theme.text_muted)
                    .child(label.to_owned()),
            )
            .child(div().flex_1().min_w_0().child(field))
            .into_any_element()
    }

    /// Which rename the band is drawing the name field for, if any.
    ///
    /// One field, and it can only be in one place: the sidebar row for the same
    /// entry asks this before drawing it, so the two never both claim it. The
    /// band wins because it is the one still on screen with the sidebar folded
    /// away, which is where renaming from the `···` would otherwise go nowhere.
    pub(crate) fn header_renaming(&self, cx: &App) -> Option<&Renaming> {
        let at = self.renaming.as_ref()?;
        let showing = self.toolbar(self.showing(cx)?, cx)?;
        let Naming::Inline(shown) = showing.entry?.naming else {
            return None;
        };
        (shown == *at).then_some(at)
    }

    /// The band itself, drawn over the pane rather than above it: the content
    /// scrolls under the glass, which is how `../desktop` has it (`app.css`)
    /// and how the composer already sits on the other edge.
    pub(crate) fn pane_header(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::of(cx).clone();
        let toolbar = self.showing(cx).and_then(|pane| self.toolbar(pane, cx));
        // The lights belong to the window, not to a pane, so the room they need
        // is taken here and nowhere a pane can see it — with the sidebar open
        // they sit over the sidebar's own header instead.
        let inset = match self.sidebar_open || window.is_fullscreen() {
            true => root::HEADER_INSET,
            false => root::TOOLBAR_INSET,
        };
        let renaming = self.header_renaming(cx).is_some();
        // Which board the band is showing, if it is showing one — the only
        // entry whose identity is two fields and so the only one with a panel.
        let board = toolbar.as_ref().and_then(|toolbar| match &toolbar.entry {
            Some(Entry {
                naming: Naming::Panel(id),
                ..
            }) => Some(id.clone()),
            _ => None,
        });
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(root::HEADER_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .pl(px(inset))
            .pr(px(root::HEADER_INSET))
            // Above the pane, which runs under it.
            // The fold lives on whichever column runs along the window's left
            // edge, so with the sidebar gone it is this one's — and it is the
            // band's first item rather than something floating at its height.
            .children(
                (!self.sidebar_open).then(|| self.fold_toggle(theme.text, cx).into_any_element()),
            )
            .children(toolbar.map(|toolbar| {
                match renaming {
                    true => div().flex_1().min_w_0().child(self.name_field(cx)),
                    false => div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.))
                        // A board's name is the way into its identity panel,
                        // the way `../desktop` opens a project's from the name
                        // it shows: what a thing is called and what it answers
                        // to are one panel, opened from the thing they name.
                        // The other panes are named by one field and rename
                        // from the `···`.
                        .child(
                            div()
                                .relative()
                                .min_w_0()
                                .flex_1()
                                // A row, so the name is only as wide as it
                                // reads. Stretched to the band's whole width it
                                // would take every press that landed on the
                                // empty half of the header with it.
                                .flex()
                                .flex_row()
                                .items_center()
                                .child(
                                    div()
                                        .id("header-title")
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_style(TextStyle::Subheadline)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text)
                                        .when_some(board.clone(), |title, _| title.cursor_pointer())
                                        .child(toolbar.title)
                                        .when_some(board.clone(), |title, id| {
                                            // Noted on the way down, ahead of
                                            // the panel's own dismiss — see
                                            // [`Cydonia::toggle_info`], which
                                            // is [`Cydonia::toggle_menu`]'s
                                            // problem and its answer.
                                            let opened = id.clone();
                                            title
                                                .capture_any_mouse_down(cx.listener(
                                                    move |this, _, _, _| {
                                                        this.info_pressed =
                                                            this.info.as_ref().is_some_and(
                                                                |info| info.board == opened,
                                                            );
                                                    },
                                                ))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.toggle_info(&id, window, cx);
                                                    },
                                                ))
                                        }),
                                )
                                .children(board.as_deref().and_then(|id| self.info_panel(id, cx))),
                        )
                        .children(toolbar.entry.map(|entry| {
                            self.menu_button(
                                SharedString::from("header-menu"),
                                // On show, not behind a hover. There is one of
                                // these on screen and it belongs to the pane in
                                // front of you — hiding a trigger until the
                                // pointer finds it is what a list of rows
                                // needs, not what a band with a single control
                                // does.
                                None,
                                icons::icon(icons::system::MENU_DOTS)
                                    .size(px(14.))
                                    .text_color(theme.text_faint),
                                Menu::Header,
                                cx,
                            )
                            .children(self.entry_menu(
                                Menu::Header,
                                entry.row,
                                entry.archived,
                                cx,
                            ))
                        })),
                }
            }))
            .into_any_element()
    }
}
