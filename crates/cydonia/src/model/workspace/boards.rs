//! The boards a project holds, and the columns and cards on them.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
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
        let open = self
            .projects
            .get_mut(project)
            .ok_or("That project is not open.".to_owned())?;
        // The same reach as [`Self::edit_board`]: a handle is heard by an agent
        // running in this project, so that is as far as it has to carry.
        if open.boards.iter().any(|board| board.key == key) {
            return Err(format!("{key} is another board's key here."));
        }
        let board = open
            .store()
            .create_board(name.trim(), &key)
            .ok_or("The board could not be written.".to_owned())?;
        open.boards.insert(0, board);
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

    pub fn active_board_mut(&mut self) -> Option<&mut Board> {
        if !self.settings.features.boards {
            return None;
        }
        let project = self.projects.get_mut(self.active?)?;
        project.boards.get_mut(project.board?)
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
    /// A lane on the open board, answered by its id so the pane can open it
    /// straight into its name.
    pub fn new_column(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let id = self
            .active_board_mut()?
            .add_column(artifact::board::column::NAMED)
            .id
            .clone();
        self.save_board();
        cx.notify();
        Some(id)
    }

    pub fn rename_column(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        let renamed = self
            .active_board_mut()
            .is_some_and(|board| board.rename_column(id, name.trim()));
        if renamed {
            self.save_board();
        }
        cx.notify();
    }

    /// Drop a lane, which a board refuses while it still holds cards — see
    /// [`Board::remove_column`].
    pub fn remove_column(&mut self, id: &str, cx: &mut Context<Self>) {
        let gone = self
            .active_board_mut()
            .is_some_and(|board| board.remove_column(id));
        if gone {
            self.save_board();
        }
        cx.notify();
    }

    fn with_board(&mut self, id: &str, edit: impl FnOnce(&fs::Project, &mut Board)) {
        for open in &mut self.projects {
            if !open.load_board(id) {
                return;
            }
            let store = open.store();
            if let Some(board) = open.boards.iter_mut().find(|board| board.id == id) {
                edit(&store, board);
                return;
            }
        }
    }

    /// Write the open board back, for an edit the pane made in place.
    pub fn save_board(&mut self) {
        let Some(open) = self.active.and_then(|ix| self.projects.get_mut(ix)) else {
            return;
        };
        if let Some(id) = open
            .board
            .and_then(|ix| open.boards.get(ix))
            .map(|board| board.id.clone())
            && !open.load_board(&id)
        {
            return;
        }
        let store = open.store();
        if let Some(board) = open.board.and_then(|ix| open.boards.get_mut(ix)) {
            store.save_board(board);
        }
    }
}
