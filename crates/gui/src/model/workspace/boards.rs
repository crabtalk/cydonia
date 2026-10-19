//! The boards a project holds, and the columns and cards on them.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use artifact::board::{
    self,
    blocking::{self, Counterpart, Edit, Place},
    key,
};
use artifact::error::{Act, Error};
use artifact::kind::Kind;
use artifact::project::Project as _;
use bezel::gpui::{Entity, Task};
use std::collections::HashSet;

impl Workspace {
    /// Save a draft of `page` against the latest disk contents, preserving
    /// the rest of the board.
    ///
    /// Refused at once where the draft does not go onto the board as it is
    /// read. Otherwise the save is issued, and the task answers whether it
    /// landed.
    pub fn save_page_draft(
        &mut self,
        member: &artifact::space::Entry,
        page: &board::Page,
        base: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<Task<Result<(), String>>, String> {
        let open = self
            .projects
            .iter()
            .find(|open| open.location() == member.project)
            .ok_or("The project is no longer open.")?;
        let store = open.store();
        let held = open
            .boards
            .iter()
            .find(|held| held.id == member.id)
            .map(|held| held.board.clone());
        let mut board = store
            .board(&member.id)
            .ok_or("The board could not be read.")?;
        if let Err(error) = apply_page_draft(&mut board, page, base, text) {
            if let Some(held) = &held {
                held.update(cx, |held, cx| {
                    *held = board;
                    cx.notify();
                });
            }
            return Err(error);
        }
        let saved = store.save_board(&mut board);
        if let Some(held) = &held {
            held.update(cx, |held, cx| {
                *held = board;
                cx.notify();
            });
        }
        let (id, page, base, text) = (
            member.id.clone(),
            page.clone(),
            base.to_owned(),
            text.to_owned(),
        );
        Ok(cx.spawn(async move |_, cx| {
            let answer = match saved.await {
                // Twice at most: a save refused as stale is another writer
                // landing between the read and the write, and the draft goes
                // onto theirs.
                Err(Error::Stale) => {
                    let mut board = store.board(&id).ok_or("The board could not be read.")?;
                    if let Err(error) = apply_page_draft(&mut board, &page, &base, &text) {
                        if let Some(held) = &held {
                            held.update(cx, |held, cx| {
                                *held = board;
                                cx.notify();
                            });
                        }
                        return Err(error);
                    }
                    let answer = store.save_board(&mut board).await;
                    if answer.is_ok()
                        && let Some(held) = &held
                    {
                        held.update(cx, |held, cx| {
                            *held = board;
                            cx.notify();
                        });
                    }
                    answer
                }
                answer => answer,
            };
            if answer.is_ok() {
                return Ok(());
            }
            // What the backend holds, rather than the draft it refused.
            if let (Some(held), Some(board)) = (&held, store.board(&id)) {
                held.update(cx, |held, cx| {
                    *held = board;
                    cx.notify();
                });
            }
            Err(match page {
                board::Page::Card(_) => "The card could not be saved. Your draft is retained.",
                board::Page::Description => {
                    "The description could not be saved. Your draft is retained."
                }
            }
            .into())
        }))
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
    /// [`Self::edit_board`] does. Answers the board's project and its place
    /// in that project's boards once the backend has made it.
    pub fn new_board(
        &mut self,
        project: usize,
        name: String,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Task<Result<(usize, usize), String>> {
        if !self.settings.features.boards {
            return Task::ready(Err("Boards are switched off.".to_owned()));
        }
        let view = self.board_view;
        let Some(open) = self.projects.get(project) else {
            return Task::ready(Err("That project is not open.".to_owned()));
        };
        // A handle is heard by an agent running in this project, so that is as
        // far as a key has to be unique.
        let held: Vec<Board> = open
            .boards
            .iter()
            .map(|held| held.board.read(cx).clone())
            .collect();
        let key = match artifact::board::key::claim(key, &held, None) {
            Ok(key) => key,
            Err(e) => return Task::ready(Err(e.to_string())),
        };
        let (store, path) = (open.store(), open.path.clone());
        let name = name.trim().to_owned();
        let made = store.create_board(&name, &key);
        cx.spawn(async move |workspace, cx| {
            let mut board = made
                .await
                .map_err(|e| Error::failed(&name, Act::Written, e).to_string())?;
            // What the app is set to, written into the board as it is made —
            // see [`artifact::board::Board::view`]. From here the board answers
            // for itself, and the setting moving does not move it.
            board.view = view;
            let _ = store.save_board(&mut board).await;
            workspace
                .update(cx, |workspace, cx| {
                    let project = workspace
                        .projects
                        .iter()
                        .position(|open| open.path == path)
                        .ok_or("That project is not open.".to_owned())?;
                    workspace.projects[project]
                        .boards
                        .insert(0, crate::model::project::HeldBoard::new(board, cx));
                    workspace.open_board(project, 0, cx);
                    Ok((project, 0))
                })
                .map_err(|e| e.to_string())?
        })
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
        let Some(id) = self
            .projects
            .get(project)
            .and_then(|open| open.boards.get(ix))
            .map(|held| held.id.clone())
        else {
            return;
        };
        let edits: Vec<Edit> = self
            .board_ends(&id, cx)
            .into_iter()
            .flat_map(|(place, _, others)| blocking::forget(&place, &others))
            .collect();
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        cx.background_spawn(open.store().remove_board(&open.boards.remove(ix).id))
            .detach();
        open.board = open
            .board
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        self.write_edits(&edits, cx);
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
        // Found under the old key: every card naming one of this board's cards
        // names it again under the new one.
        let ends = self.board_ends(id, cx);
        if self.board_entity(id).is_none() {
            return Err("The board could not be read.".to_owned());
        }
        self.with_board(id, cx, {
            let (name, key) = (name.clone(), key.clone());
            move |board| {
                board.name = name.clone();
                board.key = key.clone();
            }
        })
        .detach();
        let edits: Vec<Edit> = ends
            .iter()
            .filter(|(from, ..)| from.key != key)
            .flat_map(|(from, card, others)| {
                let to = Place {
                    key: key.clone(),
                    ..from.clone()
                };
                blocking::moved(from, &to, id, card, others)
            })
            .collect();
        self.write_edits(&edits, cx);
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
        self.with_board(id, cx, move |board| {
            board.view = view;
        })
        .detach();
    }

    pub fn archive_board(&mut self, id: &str, archived: bool, cx: &mut Context<Self>) {
        self.with_board(id, cx, move |board| {
            board.archived = archived;
        })
        .detach();
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
    /// A lane at the end of one board, answered by its id once it is saved,
    /// so the pane can open it straight into its name.
    pub fn new_column(&mut self, board: &str, cx: &mut Context<Self>) -> Task<Option<String>> {
        self.with_board(board, cx, |board| {
            board.add_column(artifact::board::column::NAMED).id.clone()
        })
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
    ) -> Task<Option<String>> {
        let id = id.to_owned();
        let minted = self.with_board(board, cx, move |board| {
            let minted = board
                .add_column_beside(artifact::board::column::NAMED, &id, after)?
                .id
                .clone();
            Some(minted)
        });
        cx.spawn(async move |_, _| minted.await.flatten())
    }

    pub fn rename_column(&mut self, board: &str, id: &str, name: String, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.with_board(board, cx, move |board| {
            board.rename_column(&id, name.trim());
        })
        .detach();
    }

    /// Step a lane one place along, by the lane it lands in front of — see
    /// [`Board::move_column_before`]. `None` at either end is a lane already
    /// where it is being asked to go.
    pub fn move_column(&mut self, board: &str, id: &str, step: isize, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.with_board(board, cx, move |board| {
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
            board.move_column_before(&id, before.as_deref());
        })
        .detach();
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
        let (id, before) = (id.to_owned(), before.map(str::to_owned));
        self.with_board(board, cx, move |board| {
            board.move_column_before(&id, before.as_deref());
        })
        .detach();
    }

    /// Fold a lane shut in the list view, or open it back up. Named by the
    /// board it sits on rather than taken from the active one: a space can
    /// have two boards on screen, and the lane pressed is not always on the
    /// one in front.
    pub fn toggle_column_collapsed(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.with_board(board, cx, move |board| {
            let Some(column) = board.columns.iter_mut().find(|column| column.id == id) else {
                return;
            };
            column.collapsed = !column.collapsed;
        })
        .detach();
    }

    /// Drop a lane, which a board refuses while it still holds cards — see
    /// [`Board::remove_column`].
    pub fn remove_column(&mut self, board: &str, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.with_board(board, cx, move |board| {
            board.remove_column(&id);
        })
        .detach();
    }

    /// Change the board an id names, wherever it is open, and write it back
    /// when the edit changed it. The task answers, once the save is answered,
    /// what the edit did on the run that was saved; nothing where no open
    /// project holds that board. Dropping the task drops a retry.
    ///
    /// A save answered [`Error::Stale`] — another writer saved the board since
    /// it was read — takes the board as the backend then holds it, applies the
    /// same edit to that, and saves it once more. A board removed in the
    /// meantime keeps the edit in memory only, and the task answers the first
    /// run.
    ///
    /// Every write to a board goes through here, named by the board: the
    /// window can have two on screen, and the project's own selection answers
    /// for at most one of them.
    pub(super) fn with_board<T: 'static>(
        &mut self,
        id: &str,
        cx: &mut App,
        mut edit: impl FnMut(&mut Board) -> T + 'static,
    ) -> Task<Option<T>> {
        for open in &mut self.projects {
            let store = open.store();
            let Some(held) = open.boards.iter().find(|held| held.id == id) else {
                continue;
            };
            let id = id.to_owned();
            return held.board.update(cx, |board, cx| {
                let before = serde_json::to_value(&*board).ok();
                let done = edit(board);
                if serde_json::to_value(&*board).ok() == before {
                    return Task::ready(Some(done));
                }
                let saved = store.save_board(board);
                cx.notify();
                cx.spawn(async move |board, cx| {
                    if !matches!(saved.await, Err(Error::Stale)) {
                        return Some(done);
                    }
                    let Some(mut fresh) = store.board(&id) else {
                        return Some(done);
                    };
                    let done = edit(&mut fresh);
                    let _ = store.save_board(&mut fresh).await;
                    let _ = board.update(cx, |board, cx| {
                        *board = fresh;
                        cx.notify();
                    });
                    Some(done)
                })
            });
        }
        Task::ready(None)
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
    ) -> Task<Option<bool>> {
        let Some(open) = self.projects.get_mut(project) else {
            return Task::ready(None);
        };
        let Some(id) = open.boards.get(ix).map(|board| board.id.clone()) else {
            return Task::ready(None);
        };
        let (card, column, before) = (
            card.to_owned(),
            column.to_owned(),
            before.map(str::to_owned),
        );
        self.with_board(&id, cx, move |board| {
            board.move_card_before(&card, &column, before.as_deref())
        })
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
        cx: &mut Context<Self>,
    ) -> Task<Option<String>> {
        if from == to {
            return Task::ready(None);
        }
        let (Some(mut source), Some(mut landing)) =
            (self.board_copy(from, cx), self.board_copy(to, cx))
        else {
            return Task::ready(None);
        };
        let ends = self.card_ends(&source.id, card, cx);
        let Some(mut landed) = artifact::board::carry_card(&mut source, &mut landing, card, column)
        else {
            return Task::ready(None);
        };
        let (Some(source_store), Some(landing_store)) = (
            self.projects.get(from.0).map(|open| open.store()),
            self.projects.get(to.0).map(|open| open.store()),
        ) else {
            return Task::ready(None);
        };
        // The landing is written first: refused there, nothing has moved, and
        // the carry is made again over both boards as their backends hold
        // them now. Refused at the source after the landing took the card is
        // a writer landing between the two saves, which is not retried.
        let carried = source_store.carry_card_files(
            &source.id,
            card,
            &landing_store,
            &landing.id,
            &landed.id,
        );
        let saved = landing_store.save_board(&mut landing);
        let (card, column) = (card.to_owned(), column.map(str::to_owned));
        cx.spawn(async move |workspace, cx| {
            let _ = carried.await;
            if let Err(Error::Stale) = saved.await {
                source = source_store.board(&source.id)?;
                landing = landing_store.board(&landing.id)?;
                landed = artifact::board::carry_card(
                    &mut source,
                    &mut landing,
                    &card,
                    column.as_deref(),
                )?;
                let _ = source_store
                    .carry_card_files(&source.id, &card, &landing_store, &landing.id, &landed.id)
                    .await;
                let _ = landing_store.save_board(&mut landing).await;
            }
            let _ = source_store.save_board(&mut source).await;
            workspace
                .update(cx, |workspace, cx| {
                    workspace.carried(from, to, (source, landing), landed, ends, cx)
                })
                .ok()
                .flatten()
        })
    }

    /// Take a carry's two boards as written, and name the card again where it
    /// landed on every card naming it. Answers what it is called there.
    fn carried(
        &mut self,
        from: (usize, usize),
        to: (usize, usize),
        (source, landing): (Board, Board),
        landed: artifact::board::Landed,
        ends: Option<(Place, Vec<Counterpart>)>,
        cx: &mut App,
    ) -> Option<String> {
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
        if let Some((from, others)) = ends
            && let Some(path) = self.projects.get(to.0).map(|open| open.path.clone())
            && let Some(landing) = self
                .projects
                .get(to.0)
                .and_then(|open| open.board(to.1, cx))
            && let Some(handle) = landing.card(&landed.id).and_then(|card| card.handle)
        {
            let at = Place {
                project: path,
                key: landing.key.clone(),
                handle,
            };
            let edits = blocking::moved(&from, &at, &landing.id.clone(), &landed.id, &others);
            self.write_edits(&edits, cx);
        }
        Some(landed.said)
    }

    /// A copy of the board at `(project, ix)`.
    fn board_copy(&self, (project, ix): (usize, usize), cx: &App) -> Option<Board> {
        self.projects.get(project)?.board(ix, cx).cloned()
    }

    /// Give card `card` of board `board` exactly `wanted` to wait on, each as
    /// its project writes it, and name it back on each of them — see
    /// [`blocking::set_blockers`]. Answers the first of `wanted` that names no
    /// card.
    pub fn set_card_blockers(
        &mut self,
        board: &str,
        card: &str,
        wanted: &[String],
        cx: &mut App,
    ) -> Result<(), String> {
        let Some((place, held)) = self.held_card(board, card, cx, |place, card| {
            (place, card.blocked_by.clone())
        }) else {
            return Ok(());
        };
        let edits = blocking::set_blockers(&self.shelf(cx), &place, board, card, &held, wanted)?;
        self.write_edits(&edits, cx);
        Ok(())
    }

    /// Take card `card` off board `board`, and off the lists of every card
    /// naming it, once the board's save says it was there. Answers whether it
    /// was.
    pub fn remove_card(&mut self, board: &str, card: &str, cx: &mut Context<Self>) -> Task<bool> {
        let edits = self
            .card_ends(board, card, cx)
            .map(|(place, others)| blocking::forget(&place, &others))
            .unwrap_or_default();
        let removed = self.with_board(board, cx, {
            let card = card.to_owned();
            move |held| held.remove_card(&card).is_some()
        });
        cx.spawn(async move |workspace, cx| {
            let removed = removed.await.unwrap_or(false);
            if removed {
                let _ = workspace.update(cx, |workspace, cx| workspace.write_edits(&edits, cx));
            }
            removed
        })
    }

    /// Where card `card` of board `board` is, and `read` of it.
    fn held_card<T>(
        &self,
        board: &str,
        card: &str,
        cx: &App,
        read: impl FnOnce(Place, &artifact::board::Card) -> T,
    ) -> Option<T> {
        let open = self
            .projects
            .iter()
            .find(|open| open.boards.iter().any(|held| held.id == board))?;
        let held = open.boards.iter().find(|held| held.id == board)?;
        let board = held.board.read(cx);
        let card = board.card(card)?;
        Some(read(Place::of(&open.path, board, card)?, card))
    }

    /// Where card `card` of board `board` is, and the cards its lists name —
    /// read before it moves or goes.
    fn card_ends(&self, board: &str, card: &str, cx: &App) -> Option<(Place, Vec<Counterpart>)> {
        let shelf = self.shelf(cx);
        self.held_card(board, card, cx, |place, card| {
            let others = blocking::counterparts(&shelf, &place.project, card);
            (place, others)
        })
    }

    /// [`Self::card_ends`] for every card of board `board`, with each card's
    /// id.
    fn board_ends(&self, board: &str, cx: &App) -> Vec<(Place, String, Vec<Counterpart>)> {
        let Some(held) = self
            .projects
            .iter()
            .flat_map(|open| &open.boards)
            .find(|held| held.id == board)
        else {
            return Vec::new();
        };
        held.board
            .read(cx)
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .filter_map(|card| {
                let (place, others) = self.card_ends(board, &card.id, cx)?;
                Some((place, card.id.clone(), others))
            })
            .collect()
    }

    /// Lay `edits` on the boards they land on, each through
    /// [`Self::with_board`].
    fn write_edits(&mut self, edits: &[Edit], cx: &mut App) {
        for (path, id) in blocking::boards(edits) {
            let edits = edits.to_vec();
            self.with_board(&id, cx, move |board| blocking::apply(&edits, &path, board))
                .detach();
        }
    }

    /// Change one board, named by the board, and write it back.
    ///
    /// What a pane's own edits go through — see
    /// [`crate::model::workspace::Workspace::board_of`] for how a pane names
    /// the board it is showing. Answers as [`Self::with_board`] does.
    ///
    /// The edit may run twice — see [`Self::with_board`].
    pub fn write_board<T: 'static>(
        &mut self,
        id: &str,
        cx: &mut App,
        edit: impl FnMut(&mut Board) -> T + 'static,
    ) -> Task<Option<T>> {
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
