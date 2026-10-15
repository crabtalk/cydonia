//! Which cards wait on which. A card names the cards it waits on in
//! [`Card::blocked_by`], and each of those names it back in [`Card::blocks`].
//!
//! The two sides change together. The functions here work out the [`Edit`]s a
//! change makes on every board it touches, and the caller applies them to the
//! boards its backend holds, each through its own write. Every card is named as
//! the project holding the list writes it — see [`Place::written`].

use crate::{
    board::{Board, Card},
    reference::{self, Reference, Target},
};
use std::path::{Path, PathBuf};

/// A card wherever it is: its project's directory, its board's key and its
/// handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub project: PathBuf,
    pub key: String,
    pub handle: u64,
}

impl Place {
    /// `card` on `board`, a board of the project at `project`. `None` for a
    /// card with no handle or a board with no key, which nothing can name.
    pub fn of(project: &Path, board: &Board, card: &Card) -> Option<Self> {
        let handle = card.handle?;
        (!board.key.is_empty()).then(|| Self {
            project: project.to_path_buf(),
            key: board.key.clone(),
            handle,
        })
    }

    /// As the project at `from` writes it: `DEV-12` there, `foo#DEV-12`
    /// elsewhere.
    pub fn written(&self, from: &Path) -> String {
        let name = reference::project_name(&self.project).unwrap_or_default();
        Reference {
            project: Some(&name),
            target: Target::Card {
                key: &self.key,
                handle: self.handle,
            },
        }
        .relative(self.project == from)
        .to_string()
    }
}

/// The open projects and their boards, as the caller's backend holds them.
pub trait Shelf {
    /// The open project `name` names, as written in the project at `from`;
    /// `from` itself for none.
    fn project(&self, from: &Path, name: Option<&str>) -> Option<PathBuf>;

    /// The board keyed `key` in the open project at `project`, and the id of
    /// its card `handle`.
    fn card(&self, project: &Path, key: &str, handle: u64) -> Option<Found>;
}

/// A card a [`Shelf`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The board's id.
    pub board: String,
    /// The board's key, as the board writes it.
    pub key: String,
    /// The card's id.
    pub card: String,
}

/// A card at the other end of one of a card's references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterpart {
    pub place: Place,
    pub board: String,
    pub card: String,
    /// Named in the card's `blocked_by`, rather than in its `blocks`.
    pub awaited: bool,
}

/// One change to one card's lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub project: PathBuf,
    pub board: String,
    pub card: String,
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The card's `blocked_by`, whole.
    Waits(Vec<String>),
    /// Name a card in its `blocks`.
    Blocks(String),
    /// Stop naming a card in its `blocks`.
    Unblocks(String),
    /// Stop naming a card in either list.
    Forget(String),
    /// Name a card by another reference in either list.
    Rename { from: String, to: String },
    /// Both of the card's lists, whole.
    Lists {
        blocked_by: Vec<String>,
        blocks: Vec<String>,
    },
}

impl Edit {
    /// Make the change on `board`, where the card is. Says whether anything
    /// changed.
    pub fn apply(&self, board: &mut Board) -> bool {
        let Some(card) = board.card_mut(&self.card) else {
            return false;
        };
        let before = (card.blocked_by.clone(), card.blocks.clone());
        match &self.change {
            Change::Waits(list) => card.blocked_by = list.clone(),
            Change::Blocks(named) => {
                if !card.blocks.contains(named) {
                    card.blocks.push(named.clone());
                }
            }
            Change::Unblocks(named) => card.blocks.retain(|held| held != named),
            Change::Forget(named) => {
                card.blocked_by.retain(|held| held != named);
                card.blocks.retain(|held| held != named);
            }
            Change::Rename { from, to } => {
                for list in [&mut card.blocked_by, &mut card.blocks] {
                    for held in list.iter_mut().filter(|held| *held == from) {
                        *held = to.clone();
                    }
                }
            }
            Change::Lists { blocked_by, blocks } => {
                card.blocked_by = blocked_by.clone();
                card.blocks = blocks.clone();
            }
        }
        (card.blocked_by.clone(), card.blocks.clone()) != before
    }
}

/// Apply every one of `edits` that lands on `board`, a board of the project at
/// `project`. Says whether any changed it.
pub fn apply(edits: &[Edit], project: &Path, board: &mut Board) -> bool {
    let id = board.id.clone();
    let mut changed = false;
    for edit in edits
        .iter()
        .filter(|edit| edit.project == project && edit.board == id)
    {
        changed |= edit.apply(board);
    }
    changed
}

/// The boards `edits` land on, each once, by project and board id.
pub fn boards(edits: &[Edit]) -> Vec<(PathBuf, String)> {
    let mut boards: Vec<(PathBuf, String)> = Vec::new();
    for edit in edits {
        if !boards
            .iter()
            .any(|(project, board)| *project == edit.project && *board == edit.board)
        {
            boards.push((edit.project.clone(), edit.board.clone()));
        }
    }
    boards
}

/// The card `written` names, as written in the project at `from`.
pub fn locate(shelf: &impl Shelf, from: &Path, written: &str) -> Option<(Place, Found)> {
    let Reference {
        project,
        target: Target::Card { key, handle },
    } = reference::parse(written)?
    else {
        return None;
    };
    let project = shelf.project(from, project)?;
    let found = shelf.card(&project, key, handle)?;
    let place = Place {
        project,
        key: found.key.clone(),
        handle,
    };
    Some((place, found))
}

/// Every card `card`'s lists name that is still there. `project` is the
/// project holding `card`.
pub fn counterparts(shelf: &impl Shelf, project: &Path, card: &Card) -> Vec<Counterpart> {
    let awaited = card.blocked_by.iter().map(|written| (written, true));
    let waiting = card.blocks.iter().map(|written| (written, false));
    awaited
        .chain(waiting)
        .filter_map(|(written, awaited)| {
            let (place, found) = locate(shelf, project, written)?;
            Some(Counterpart {
                place,
                board: found.board,
                card: found.card,
                awaited,
            })
        })
        .collect()
}

/// Give the card at `waiter`, card `card` of board `board`, exactly `wanted`
/// to wait on, where it waited on `held`. Each of `wanted` is as the waiter's
/// project writes it; the first that names no card is answered as the error.
/// Every one of `wanted` is named back, whether or not it was held already.
pub fn set_blockers(
    shelf: &impl Shelf,
    waiter: &Place,
    board: &str,
    card: &str,
    held: &[String],
    wanted: &[String],
) -> Result<Vec<Edit>, String> {
    let mut kept: Vec<String> = Vec::new();
    let mut edits: Vec<Edit> = Vec::new();
    for written in wanted {
        let (place, found) =
            locate(shelf, &waiter.project, written).ok_or_else(|| written.clone())?;
        if place == *waiter || kept.contains(written) {
            continue;
        }
        edits.push(Edit {
            project: place.project.clone(),
            board: found.board,
            card: found.card,
            change: Change::Blocks(waiter.written(&place.project)),
        });
        kept.push(written.clone());
    }
    for written in held.iter().filter(|held| !kept.contains(held)) {
        if let Some((place, found)) = locate(shelf, &waiter.project, written) {
            edits.push(Edit {
                project: place.project.clone(),
                board: found.board,
                card: found.card,
                change: Change::Unblocks(waiter.written(&place.project)),
            });
        }
    }
    edits.push(Edit {
        project: waiter.project.clone(),
        board: board.to_owned(),
        card: card.to_owned(),
        change: Change::Waits(kept),
    });
    Ok(edits)
}

/// The card at `gone` is going: every card naming it stops.
pub fn forget(gone: &Place, counterparts: &[Counterpart]) -> Vec<Edit> {
    counterparts
        .iter()
        .map(|other| Edit {
            project: other.place.project.clone(),
            board: other.board.clone(),
            card: other.card.clone(),
            change: Change::Forget(gone.written(&other.place.project)),
        })
        .collect()
}

/// The card at `from` is now at `to`, card `card` of board `board`: carried to
/// another board, or its board re-keyed. Every card naming it names it anew,
/// and where it changed project its own lists are written again as `to`'s
/// project writes them. `counterparts` were found before it moved.
pub fn moved(
    from: &Place,
    to: &Place,
    board: &str,
    card: &str,
    counterparts: &[Counterpart],
) -> Vec<Edit> {
    let mut edits: Vec<Edit> = counterparts
        .iter()
        .map(|other| Edit {
            project: other.place.project.clone(),
            board: other.board.clone(),
            card: other.card.clone(),
            change: Change::Rename {
                from: from.written(&other.place.project),
                to: to.written(&other.place.project),
            },
        })
        .collect();
    if from.project == to.project {
        return edits;
    }
    let written = |awaited: bool| -> Vec<String> {
        counterparts
            .iter()
            .filter(|other| other.awaited == awaited)
            .map(|other| other.place.written(&to.project))
            .collect()
    };
    edits.push(Edit {
        project: to.project.clone(),
        board: board.to_owned(),
        card: card.to_owned(),
        change: Change::Lists {
            blocked_by: written(true),
            blocks: written(false),
        },
    });
    edits
}
