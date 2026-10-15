//! The boards a project holds, and the columns and cards on them.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use artifact::board::{self, key};
use artifact::error::{Act, Error};
use artifact::kind::Kind;
use artifact::project::Project as _;
use bezel::gpui::Entity;
use std::collections::HashSet;

impl Workspace {
    /// Save a draft of `page` against the latest disk contents, preserving
    /// the rest of the board.
    pub fn save_page_draft(
        &mut self,
        member: &artifact::space::Entry,
        page: &board::Page,
        base: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let open = self
            .projects
            .iter_mut()
            .find(|open| open.path == member.project)
            .ok_or("The project is no longer open.")?;
        let store = open.store();
        // Twice at most: a save refused as stale is another writer landing
        // between this read and this write, and the draft goes onto theirs.
        for _ in 0..2 {
            let mut board = store
                .board(&member.id)
                .ok_or("The board could not be read.")?;
            if let Err(error) = apply_page_draft(&mut board, page, base, text) {
                if let Some(held) = open.boards.iter().find(|held| held.id == member.id) {
                    held.board.update(cx, |held, cx| {
                        *held = board;
                        cx.notify();
                    });
                }
                return Err(error);
            }
            match store.save_board(&mut board) {
                Err(Error::Stale) => continue,
                _ => break,
            }
        }
        let saved = store
            .board(&member.id)
            .ok_or("The saved board could not be read.")?;
        if saved.page_text(page) != Some(text) {
            return Err(match page {
                board::Page::Card(_) => "The card could not be saved. Your draft is retained.",
                board::Page::Description => {
                    "The description could not be saved. Your draft is retained."
                }
            }
            .into());
        }
        if let Some(held) = open.boards.iter().find(|held| held.id == member.id) {
            held.board.update(cx, |held, cx| {
                *held = saved;
                cx.notify();
            });
        }
        Ok(())
    }

    /// The key a board called `name` would take in `project`, clear of the
    /// keys its boards hold. A blank name is keyed as [`board::NAMED`].
    pub fn free_key(&self, project: usize, name: &str, cx: &App) -> String {
        let taken: HashSet<String> = self
            .projects
            .get(project)
            .map(|open| {
                open.boards
                    .iter()
                    .map(|held| held.board.read(cx).key.clone())
                    .collect()
            })
            .unwrap_or_default();
        let name = match name.trim() {
            "" => board::NAMED,
            typed => typed,
        };
        key::derive(name, &taken)
    }

    /// A board in `project`, called and keyed as the dialog that asked for it
    /// has them, and opened as it lands. Gated here as well as in the menus
    /// that call it: this is where a board is born.
    ///
    /// Answers what is wrong rather than making the board anyway — a key that
    /// is taken is something the dialog stays open to say, the way
    /// [`Self::edit_board`] does.
    pub fn new_board(
        &mut self,
        project: usize,
        name: String,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        if !self.settings.features.boards {
            return Err("Boards are switched off.".to_owned());
        }
        // Read before the project is taken: the seed is the workspace's and the
        // borrow below is over the whole of it.
        let view = self.board_view;
        let open = self
            .projects
            .get_mut(project)
            .ok_or("That project is not open.".to_owned())?;
        // A handle is heard by an agent running in this project, so that is as
        // far as a key has to be unique.
        let held: Vec<Board> = open
            .boards
            .iter()
            .map(|held| held.board.read(cx).clone())
            .collect();
        let key = artifact::board::key::claim(key, &held, None).map_err(|e| e.to_string())?;
        let mut board = open
            .store()
            .create_board(name.trim(), &key)
            .map_err(|e| Error::failed(name.trim(), Act::Written, e).to_string())?;
        // What the app is set to, written into the board as it is made — see
        // [`artifact::board::Board::view`]. From here the board answers for
        // itself, and the setting moving does not move it.
        board.view = view;
        let _ = open.store().save_board(&mut board);
        open.boards
            .insert(0, crate::model::project::HeldBoard::new(board, cx));
        self.open_board(project, 0, cx);
        Ok(0)
    }

    /// Every project's boards are on show, so picking one brings its project
    /// forward with it.
    pub fn open_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= open.boards.len() {
            return;
        }
        open.board = Some(ix);
        let id = open.boards[ix].id.clone();
        self.active = Some(project);
        self.remember(project, Kind::Board, id, cx);
        cx.notify();
    }

    /// Drop the board: the file goes with it.
    pub fn delete_board(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= project.boards.len() {
            return;
        }
        let _ = project.store().remove_board(&project.boards.remove(ix).id);
        project.board = project
            .board
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        cx.notify();
    }

    /// Take the board's identity whole, as the header's panel gives it: what it
    /// is called, and the key its handles carry.
    ///
    /// Answers what is wrong rather than quietly keeping the old one — a key
    /// that is taken is something the panel stays open to say.
    ///
    /// Re-keying renames every handle on the board: `ROAD-12` becomes
    /// `BACK-12`. That is the cost of letting the key be edited at all, and it
    /// is the caller's to accept — the number is what does not move.
    pub fn edit_board(
        &mut self,
        id: &str,
        name: String,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(open) = self
            .projects
            .iter()
            .find(|open| open.boards.iter().any(|held| held.id == id))
        else {
            return Ok(());
        };
        let held: Vec<Board> = open
            .boards
            .iter()
            .map(|held| held.board.read(cx).clone())
            .collect();
        let key = artifact::board::key::claim(key, &held, Some(id)).map_err(|e| e.to_string())?;
        let name = name.trim().to_owned();
        self.with_board(id, cx, |board| {
            board.name = name.clone();
            board.key = key.clone();
        })
        .ok_or("The board could not be read.")?;
        self.prune_archived(cx);
        cx.notify();
        Ok(())
    }

    /// Lay one board out the other way — see [`artifact::board::View`]. By id,
    /// because the pane that asks may be showing a board other than the one in
    /// front.
    pub fn set_board_view(
        &mut self,
        id: &str,
        view: artifact::board::View,
        cx: &mut Context<Self>,
    ) {
        self.with_board(id, cx, |board| {
            board.view = view;
        });
    }

    pub fn archive_board(&mut self, id: &str, archived: bool, cx: &mut Context<Self>) {
        self.with_board(id, cx, |board| {
            board.archived = archived;
        });
        self.prune_archived(cx);
        cx.notify();
    }

    /// The board the board pane would show, and the choke point the boards
    /// switch bites at: with nothing to hand back, the pane is unreachable —
    /// nothing to render, nothing to step to, nothing for the sidebar to light.
    /// The files stay where they are.
    pub fn active_board<'a>(&self, cx: &'a App) -> Option<&'a Board> {
        if !self.settings.features.boards {
            return None;
        }
        let project = self.active_project()?;
        project.board(project.board?, cx)
    }

    /// The board a file names, wherever it is open. What a rename holds onto:
    /// an index moves the moment a neighbour is made or dropped, and the file
    /// is the board — it is where [`artifact::project::Project::save_board`] writes.
    pub fn board_at<'a>(&self, id: &str, cx: &'a App) -> Option<&'a Board> {
        Some(self.board_entity(id)?.read(cx))
    }

    /// The board a file names, as the entity every view drawing it observes.
    pub fn board_entity(&self, id: &str) -> Option<&Entity<Board>> {
        self.projects
            .iter()
            .flat_map(|open| open.boards.iter())
            .find(|held| held.id == id)
            .map(|held| &held.board)
    }

    /// Reach a board wherever it is open, with the store that holds it — the
    /// project it is in, and the only thing that can write it back.
    /// A lane at the end of one board, answered by its id so the pane can open
    /// it straight into its name.
    pub fn new_column(&mut self, board: &str, cx: &mut Context<Self>) -> Option<String> {
        let id = self.with_board(board, cx, |board| {
            board.add_column(artifact::board::column::NAMED).id.clone()
        })?;
        Some(id)
    }

    /// A lane beside the one named, on the side given — `after` for the lane
    /// that follows it in the board's own order of its columns, which runs
    /// across the lanes and down the list.
    pub fn new_column_beside(
        &mut self,
        board: &str,
        id: &str,
        after: bool,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let minted = self.with_board(board, cx, |board| {
            let minted = board
                .add_column_beside(artifact::board::column::NAMED, id, after)?
                .id
                .clone();
            Some(minted)
        })??;
        Some(minted)
    }

    pub fn rename_column(&mut self, board: &str, id: &str, name: String, cx: &mut Context<Self>) {
        self.with_board(board, cx, |board| {
            board.rename_column(id, name.trim());
        });
    }

    /// Step a lane one place along, by the lane it lands in front of — see
    /// [`Board::move_column_before`]. `None` at either end is a lane already
    /// where it is being asked to go.
    pub fn move_column(&mut self, board: &str, id: &str, step: isize, cx: &mut Context<Self>) {
        self.with_board(board, cx, |board| {
            let Some(at) = board.columns.iter().position(|column| column.id == id) else {
                return;
            };
            let to = match at.checked_add_signed(step) {
                Some(to) if to < board.columns.len() => to,
                _ => return,
            };
            // The lane it lands in front of, read after the step rather than
            // before it: moving right means going in front of the one *after*
            // the neighbour it swaps with, and off the end means no anchor at
            // all.
            let before = match step > 0 {
                true => board.columns.get(to + 1).map(|column| column.id.clone()),
                false => board.columns.get(to).map(|column| column.id.clone()),
            };
            board.move_column_before(id, before.as_deref());
        });
    }

    /// Put a lane in front of `before`, or at the end for `None` — see
    /// [`Board::move_column_before`].
    pub fn move_column_before(
        &mut self,
        board: &str,
        id: &str,
        before: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.with_board(board, cx, |board| {
            board.move_column_before(id, before);
        });
    }

    /// Fold a lane shut in the list view, or open it back up. Named by the
    /// board it sits on rather than taken from the active one: a space can
    /// have two boards on screen, and the lane pressed is not always on the
    /// one in front.
    pub fn toggle_column_collapsed(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        self.with_board(board, cx, |board| {
            let Some(column) = board.columns.iter_mut().find(|column| column.id == id) else {
                return;
            };
            column.collapsed = !column.collapsed;
        });
    }

    /// Drop a lane, which a board refuses while it still holds cards — see
    /// [`Board::remove_column`].
    pub fn remove_column(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        self.with_board(board, cx, |board| {
            board.remove_column(id);
        });
    }

    /// Change the board an id names, wherever it is open, and write it back
    /// when the edit changed it — and answer whatever the edit did. Nothing
    /// where no open project holds that board.
    ///
    /// A save refused as [`Error::Stale`] — another writer saved the board since it
    /// was read — takes the board as the backend now holds it, applies the
    /// same edit to that, and saves it once more. A board removed in the
    /// meantime keeps the edit in memory only.
    ///
    /// Every write to a board goes through here, named by the board: the
    /// window can have two on screen, and the project's own selection answers
    /// for at most one of them.
    pub(super) fn with_board<T>(
        &mut self,
        id: &str,
        cx: &mut App,
        mut edit: impl FnMut(&mut Board) -> T,
    ) -> Option<T> {
        for open in &mut self.projects {
            let store = open.store();
            let Some(held) = open.boards.iter().find(|held| held.id == id) else {
                continue;
            };
            return Some(held.board.update(cx, |board, cx| {
                let before = serde_json::to_value(&*board).ok();
                let mut done = edit(board);
                if serde_json::to_value(&*board).ok() == before {
                    return done;
                }
                if let Err(Error::Stale) = store.save_board(board)
                    && let Some(mut fresh) = store.board(id)
                {
                    done = edit(&mut fresh);
                    let _ = store.save_board(&mut fresh);
                    *board = fresh;
                }
                cx.notify();
                done
            }));
        }
        None
    }

    /// Write the open board back, for an edit the pane made in place.
    /// Move a card between the lanes of one board, named by where it sits
    /// rather than by being the active one: a space can have two boards on
    /// screen, and the one dropped onto is not always the one in front.
    pub fn move_card_within(
        &mut self,
        (project, ix): (usize, usize),
        card: &str,
        column: &str,
        before: Option<&str>,
        cx: &mut App,
    ) -> bool {
        let Some(open) = self.projects.get_mut(project) else {
            return false;
        };
        let Some(id) = open.boards.get(ix).map(|board| board.id.clone()) else {
            return false;
        };
        self.with_board(&id, cx, |board| {
            board.move_card_before(card, column, before)
        })
        .unwrap_or(false)
    }

    /// Carry a card to another board, which may be in another project.
    ///
    /// Answers what it is called where it landed — `PLAN-3` — and nothing where
    /// it did not move. It arrives under a new handle; see
    /// [`artifact::board::carry_card`].
    pub fn carry_card(
        &mut self,
        from: (usize, usize),
        to: (usize, usize),
        card: &str,
        column: Option<&str>,
        cx: &mut App,
    ) -> Option<String> {
        if from == to {
            return None;
        }
        let mut source = self.board_copy(from, cx)?;
        let mut landing = self.board_copy(to, cx)?;
        let mut landed = artifact::board::carry_card(&mut source, &mut landing, card, column)?;
        // The landing is written first: refused there, nothing has moved, and
        // the carry is made again over both boards as their backends hold
        // them now. Refused at the source after the landing took the card is
        // a writer landing between the two saves, which is not retried.
        let (source_store, landing_store) = (
            self.projects.get(from.0)?.store(),
            self.projects.get(to.0)?.store(),
        );
        source_store.carry_card_files(&source.id, card, &landing_store, &landing.id, &landed.id);
        if let Err(Error::Stale) = landing_store.save_board(&mut landing) {
            source = source_store.board(&source.id)?;
            landing = landing_store.board(&landing.id)?;
            landed = artifact::board::carry_card(&mut source, &mut landing, card, column)?;
            source_store.carry_card_files(
                &source.id,
                card,
                &landing_store,
                &landing.id,
                &landed.id,
            );
            let _ = landing_store.save_board(&mut landing);
        }
        let _ = source_store.save_board(&mut source);
        for ((project, _), board) in [(from, source), (to, landing)] {
            let Some(open) = self.projects.get_mut(project) else {
                continue;
            };
            if let Some(held) = open.boards.iter().find(|held| held.id == board.id) {
                held.board.update(cx, |held, cx| {
                    *held = board;
                    cx.notify();
                });
            }
        }
        Some(landed.said)
    }

    /// A copy of the board at `(project, ix)`.
    fn board_copy(&self, (project, ix): (usize, usize), cx: &App) -> Option<Board> {
        self.projects.get(project)?.board(ix, cx).cloned()
    }

    /// Change one board, named by the board, and write it back.
    ///
    /// What a pane's own edits go through — see
    /// [`crate::model::workspace::Workspace::board_of`] for how a pane names
    /// the board it is showing. Answers what the edit did, or nothing where no
    /// open project holds that board.
    ///
    /// The edit may run twice — see [`Self::with_board`].
    pub fn write_board<T>(
        &mut self,
        id: &str,
        cx: &mut App,
        edit: impl FnMut(&mut Board) -> T,
    ) -> Option<T> {
        self.with_board(id, cx, edit)
    }
}

fn apply_page_draft(
    board: &mut Board,
    page: &board::Page,
    base: &str,
    text: &str,
) -> Result<(), String> {
    let held = board
        .page_text(page)
        .ok_or("The card was removed or moved. Your draft is retained.")?;
    if matches!(page, board::Page::Card(_)) && text.trim().is_empty() {
        return Err("A card needs some text. Use the card menu to delete it.".into());
    }
    if held != base && held != text {
        return Err(match page {
            board::Page::Card(_) => "The card changed while you were editing. Copy your draft before cancelling to reload it.",
            board::Page::Description => "The description changed while you were editing. Copy your draft before cancelling to reload it.",
        }
        .into());
    }
    board.rewrite_page(page, text);
    Ok(())
}
