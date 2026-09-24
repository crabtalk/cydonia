//! The boards a project holds, and the columns and cards on them.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
    /// Save a draft against the latest disk contents, preserving other card fields.
    pub fn save_card_draft(
        &mut self,
        member: &artifact::space::Member,
        card: &str,
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
        let mut board = store
            .board(&member.id)
            .ok_or("The board could not be read.")?;
        if let Err(error) = apply_card_draft(&mut board, card, base, text) {
            if let Some(held) = open.boards.iter_mut().find(|held| held.id == member.id) {
                *held = board;
            }
            cx.notify();
            return Err(error);
        }
        store.save_board(&mut board);
        let saved = store
            .board(&member.id)
            .ok_or("The saved board could not be read.")?;
        if saved.card(card).is_none_or(|card| card.text != text) {
            return Err("The card could not be saved. Your draft is retained.".into());
        }
        if let Some(held) = open.boards.iter_mut().find(|held| held.id == member.id) {
            *held = saved;
        }
        cx.notify();
        Ok(())
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
        let key = artifact::board::key::normalize(key)
            .ok_or("A key needs at least one letter or digit.".to_owned())?;
        // Read before the project is taken: the seed is the workspace's and the
        // borrow below is over the whole of it.
        let view = self.board_view;
        let open = self
            .projects
            .get_mut(project)
            .ok_or("That project is not open.".to_owned())?;
        // The same reach as [`Self::edit_board`]: a handle is heard by an agent
        // running in this project, so that is as far as it has to carry.
        if open.boards.iter().any(|board| board.key == key) {
            return Err(format!("{key} is another board's key here."));
        }
        let mut board = open
            .store()
            .create_board(name.trim(), &key)
            .ok_or("The board could not be written.".to_owned())?;
        // What the app is set to, written into the board as it is made — see
        // [`artifact::board::Board::view`]. From here the board answers for
        // itself, and the setting moving does not move it.
        board.view = view;
        open.store().save_board(&mut board);
        open.boards.insert(0, board);
        self.reveal_project(project, cx);
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
        let id = open.boards[ix].id.clone();
        if !open.load_board(&id) {
            return;
        }
        open.board = Some(ix);
        let id = open.boards[ix].id.clone();
        self.active = Some(project);
        self.remember(project, state::Kind::Board, id, cx);
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
        project.store().remove_board(&project.boards.remove(ix).id);
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
        let key = artifact::board::key::normalize(key)
            .ok_or("A key needs at least one letter or digit.".to_owned())?;
        let Some(open) = self
            .projects
            .iter_mut()
            .find(|open| open.boards.iter().any(|board| board.id == id))
        else {
            return Ok(());
        };
        // Among this project's boards and no further: a handle is heard by an
        // agent running in this project, so that is as far as it has to carry.
        if open
            .boards
            .iter()
            .any(|board| board.id != id && board.key == key)
        {
            return Err(format!("{key} is another board's key here."));
        }
        if !open.load_board(id) {
            return Err("The board could not be read.".into());
        }
        let store = open.store();
        if let Some(board) = open.boards.iter_mut().find(|board| board.id == id) {
            board.name = name.trim().to_owned();
            board.key = key;
            store.save_board(board);
        }
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
        self.with_board(id, |store, board| {
            board.view = view;
            store.save_board(board);
        });
        cx.notify();
    }

    pub fn archive_board(&mut self, id: &str, archived: bool, cx: &mut Context<Self>) {
        self.with_board(id, |store, board| {
            board.archived = archived;
            store.save_board(board);
        });
        self.prune_archived(cx);
        cx.notify();
    }

    /// The board the board pane would show, and the choke point the boards
    /// switch bites at: with nothing to hand back, the pane is unreachable —
    /// nothing to render, nothing to step to, nothing for the sidebar to light.
    /// The files stay where they are.
    pub fn active_board(&self) -> Option<&Board> {
        if !self.settings.features.boards {
            return None;
        }
        let project = self.active_project()?;
        project.boards.get(project.board?)
    }

    /// The board a file names, wherever it is open. What a rename holds onto:
    /// an index moves the moment a neighbour is made or dropped, and the file
    /// is the board — it is where [`artifact::project::Project::save_board`] writes.
    pub fn board_at(&self, id: &str) -> Option<&Board> {
        self.projects
            .iter()
            .flat_map(|open| open.boards.iter())
            .find(|board| board.id == id)
    }

    /// Reach a board wherever it is open, with the store that holds it — the
    /// project it is in, and the only thing that can write it back.
    /// A lane at the end of one board, answered by its id so the pane can open
    /// it straight into its name.
    pub fn new_column(&mut self, board: &str, cx: &mut Context<Self>) -> Option<String> {
        let id = self.with_board(board, |store, board| {
            let id = board.add_column(artifact::board::column::NAMED).id.clone();
            store.save_board(board);
            id
        })?;
        cx.notify();
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
        let minted = self.with_board(board, |store, board| {
            let minted = board
                .add_column_beside(artifact::board::column::NAMED, id, after)?
                .id
                .clone();
            store.save_board(board);
            Some(minted)
        })??;
        cx.notify();
        Some(minted)
    }

    pub fn rename_column(&mut self, board: &str, id: &str, name: String, cx: &mut Context<Self>) {
        self.with_board(board, |store, board| {
            if board.rename_column(id, name.trim()) {
                store.save_board(board);
            }
        });
        cx.notify();
    }

    /// Step a lane one place along, by the lane it lands in front of — see
    /// [`Board::move_column_before`]. `None` at either end is a lane already
    /// where it is being asked to go.
    pub fn move_column(&mut self, board: &str, id: &str, step: isize, cx: &mut Context<Self>) {
        self.with_board(board, |store, board| {
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
            if board.move_column_before(id, before.as_deref()) {
                store.save_board(board);
            }
        });
        cx.notify();
    }

    /// Fold a lane shut in the list view, or open it back up. Named by the
    /// board it sits on rather than taken from the active one: a space can
    /// have two boards on screen, and the lane pressed is not always on the
    /// one in front.
    pub fn toggle_column_collapsed(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        self.with_board(board, |store, board| {
            let Some(column) = board.columns.iter_mut().find(|column| column.id == id) else {
                return;
            };
            column.collapsed = !column.collapsed;
            store.save_board(board);
        });
        cx.notify();
    }

    /// Drop a lane, which a board refuses while it still holds cards — see
    /// [`Board::remove_column`].
    pub fn remove_column(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        self.with_board(board, |store, board| {
            if board.remove_column(id) {
                store.save_board(board);
            }
        });
        cx.notify();
    }

    /// Reach the board a file names, wherever it is open, with the store that
    /// holds it — and answer whatever the edit did. Nothing where no open
    /// project holds that board.
    ///
    /// Every write to a board goes through here, named by the board: the
    /// window can have two on screen, and the project's own selection answers
    /// for at most one of them.
    fn with_board<T>(
        &mut self,
        id: &str,
        edit: impl FnOnce(&fs::Project, &mut Board) -> T,
    ) -> Option<T> {
        for open in &mut self.projects {
            if !open.load_board(id) {
                return None;
            }
            let store = open.store();
            if let Some(board) = open.boards.iter_mut().find(|board| board.id == id) {
                return Some(edit(&store, board));
            }
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
    ) -> bool {
        let Some(open) = self.projects.get_mut(project) else {
            return false;
        };
        let Some(id) = open.boards.get(ix).map(|board| board.id.clone()) else {
            return false;
        };
        if !open.load_board(&id) {
            return false;
        }
        let store = open.store();
        let Some(board) = open.boards.get_mut(ix) else {
            return false;
        };
        if !board.move_card_before(card, column, before) {
            return false;
        }
        store.save_board(board);
        true
    }

    /// Carry a card to another board, which may be in another project.
    ///
    /// Answers what it is called where it landed — `PLAN-3` — and nothing where
    /// it did not move. It arrives under a new handle with no session on it;
    /// see [`artifact::board::carry_card`].
    ///
    /// A card whose session is still open does not move.
    pub fn carry_card(
        &mut self,
        from: (usize, usize),
        to: (usize, usize),
        card: &str,
        column: Option<&str>,
    ) -> Option<String> {
        if from == to {
            return None;
        }
        let live = self
            .board_in(from.0, from.1)
            .and_then(|board| board.card(card))
            .and_then(|card| card.session.clone())
            .is_some_and(|record| self.session_by_record(&record).is_some());
        if live {
            return None;
        }
        // Both boards in hand before either is touched: a board listed in the
        // sidebar may not have been read off the disk yet, and moving a card
        // onto the half of one that is in memory would write the other half
        // away.
        let mut source = self.loaded_board(from)?;
        let mut landing = self.loaded_board(to)?;
        let landed = artifact::board::carry_card(&mut source, &mut landing, card, column)?;
        for ((project, _), mut board) in [(from, source), (to, landing)] {
            let Some(open) = self.projects.get_mut(project) else {
                continue;
            };
            let store = open.store();
            store.save_board(&mut board);
            if let Some(held) = open.boards.iter_mut().find(|held| held.id == board.id) {
                *held = board;
            }
        }
        Some(landed)
    }

    /// A board by where it sits, read off the disk if it has not been yet.
    fn loaded_board(&mut self, (project, ix): (usize, usize)) -> Option<Board> {
        let open = self.projects.get_mut(project)?;
        let id = open.boards.get(ix)?.id.clone();
        open.load_board(&id).then_some(())?;
        open.boards.get(ix).cloned()
    }

    /// Change one board, named by the board, and write it back.
    ///
    /// What a pane's own edits go through — see
    /// [`crate::model::workspace::Workspace::board_of`] for how a pane names
    /// the board it is showing. Answers what the edit did, or nothing where no
    /// open project holds that board.
    pub fn write_board<T>(&mut self, id: &str, edit: impl FnOnce(&mut Board) -> T) -> Option<T> {
        self.with_board(id, |store, board| {
            let done = edit(board);
            store.save_board(board);
            done
        })
    }
}

fn apply_card_draft(board: &mut Board, id: &str, base: &str, text: &str) -> Result<(), String> {
    let card = board
        .card(id)
        .ok_or("The card was removed or moved. Your draft is retained.")?;
    if text.trim().is_empty() {
        return Err("A card needs some text. Use the card menu to delete it.".into());
    }
    if card.text != base && card.text != text {
        return Err("The card changed while you were editing. Copy your draft before cancelling to reload it.".into());
    }
    board.rewrite_card(id, text);
    Ok(())
}
