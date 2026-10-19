//! The tools an agent works a board through.
//!
//! Every one is a named operation on [`artifact::board::Board`] with
//! addressing in front of it and a rendering behind: the pane calls the same
//! functions, which is the whole reason they were moved out of the view.
//!
//! Every tool takes the project it is about, as the path of the directory the
//! work lives in — there is one server for the whole app and no notion in it of
//! which project is in front, so the call says.
//!
//! Addressing inside one is what a person would say out loud — a key, a handle,
//! a column name — because what the model is holding came out of a conversation
//! and not out of a file. Ids work everywhere a name does, for the caller that
//! kept one.
//!
//! A refusal always says what *is* there. A model that guessed the name wrong
//! can then fix it without spending a second call finding out, which is the
//! difference between a tool that costs one turn and one that costs three.

use crate::{
    tool::{Answer, Arg, Args, Outcome, Tool, Trouble},
    tools::{HITS, PROJECT, QUERY, fields, many, on_the_rail, root},
};
use artifact::error::{Act, Error};
use artifact::{
    archive::Archived,
    board::{
        Board, Card, Column,
        blocking::{self, Counterpart, Edit, Found, Place, Shelf},
    },
    project::{Project, fs},
    reference::{self, Reference, Target},
    search::{self, Block, Query, Searchable},
};
use futures::executor::block_on;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
};

/// How many cards `board_read` answers per column.
const PAGE: usize = 50;

const BOARD: Arg = Arg {
    name: "board",
    about: "The board: its project reference (#12), key (ROAD), name, or storage id.",
};
/// The same argument where several are taken at once, the way [`CARDS`] stands
/// beside [`CARD`].
const BOARDS: Arg = Arg {
    name: "board",
    about: "The board, or several: each its project reference (#12), key, name or id.",
};
/// What [`search`] looks for, across every open project rather than in one.
const SOUGHT: Arg = Arg {
    name: "board",
    about: "The board, or several: each its key (ROAD), a card handle on it (ROAD-12), or its name.",
};
/// Which way the switch goes — the same wording `article::ARCHIVED` carries,
/// because it is the same act on the other surface.
const ARCHIVED: Arg = Arg {
    name: "archived",
    about: "true to put it away, false to bring it back. Left out, it is put away.",
};
const CARD: Arg = Arg {
    name: "card",
    about: "The card: its handle (ROAD-12), or its id.",
};
/// The same argument where several are taken at once — one card, or a list of
/// them. A second const rather than a flag on [`CARD`]: the line a client reads
/// is the whole of how it learns a list is allowed here.
const CARDS: Arg = Arg {
    name: "card",
    about: "The card, or several: each its handle (ROAD-12) or its id. They may be on different boards.",
};
const COLUMN: Arg = Arg {
    name: "column",
    about: "The column: its name, or its id.",
};
const ONE_COLUMN: Arg = Arg {
    name: "column",
    about: "Read only this column, by name or id. Left out, every column is read.",
};
const AFTER: Arg = Arg {
    name: "after",
    about: "Read the cards after this one in its column, by handle (ROAD-12) or id: the next page of a column that answered more.",
};
/// The same argument where several are taken at once. A second const rather
/// than a flag on [`COLUMN`], the way [`CARDS`] stands beside [`CARD`].
const COLUMNS: Arg = Arg {
    name: "column",
    about: "The column, or several: each its name or its id.",
};
/// Where a move takes the card. Their own arguments rather than [`BOARD`] and
/// [`PROJECT`], which are where it is now.
const TO_BOARD: Arg = Arg {
    name: "to_board",
    about: "The board to move it to, by reference (#12), key, name or id. Left out, it stays on the board it is on.",
};
const TO_PROJECT: Arg = Arg {
    name: "to_project",
    about: "The project that board is in, as a directory path, which must be one cydonia has open. Left out, it is this project. Requires to_board.",
};

const BUSY: Arg = Arg {
    name: "busy",
    about: "true while working on them, false to take the tag off.",
};
const BLOCKERS: Arg = Arg {
    name: "blocked_by",
    about: "The cards it waits on, each its handle (ROAD-12), foo#ROAD-12 for a card in the open project foo, or its id. Replaces the list; an empty list clears it.",
};

const BEFORE_COLUMN: Arg = Arg {
    name: "before",
    about: "The column to put it in front of, by name or id. Left out, it goes to the right-hand end.",
};

/// The two `text` arguments and the two `name` ones carry the same key and a
/// different line: what a card says when it is made is not what it should say
/// now. A const each, so the tool that takes one names the one it takes.
const TEXT: Arg = Arg {
    name: "text",
    about: "What the card says, or what each of several cards says — they land in the order they are given.",
};
const TEXT_NOW: Arg = Arg {
    name: "text",
    about: "What the card should say now.",
};
const NAME: Arg = Arg {
    name: "name",
    about: "What the column is called, or what each of several is called — they are added in the order they are given.",
};
const NAME_NOW: Arg = Arg {
    name: "name",
    about: "What the column should be called now.",
};

const DESCRIPTION: Arg = Arg {
    name: "description",
    about: "What the board's description should say now, markdown: the guide for working its cards. Empty to take it off.",
};

const BOARD_NAME: Arg = Arg {
    name: "name",
    about: "What the new board is called.",
};
const BOARD_NAME_NOW: Arg = Arg {
    name: "name",
    about: "What the board should be called now.",
};
const KEY: Arg = Arg {
    name: "key",
    about: "A unique board key for card handles, such as ROAD. Normalized to uppercase letters and digits.",
};
/// The key where changing it is the optional half of a rename. Its own const
/// because the consequence is the whole of what a caller needs told, and
/// [`KEY`] is read by a caller who has no handles yet.
const KEY_NOW: Arg = Arg {
    name: "key",
    about: "A new key for card handles, such as ROAD. Every handle on the board is read off it, so ROAD-12 becomes BACK-12. Left out, the key does not change.",
};

pub static TOOLS: [Tool; 20] = [
    Tool {
        name: "board_add",
        description: "Create a board with a name and unique key. Returns its id, project number, key, and columns. Use board_add_column to add columns.",
        schema: |bound| fields(bound, &[PROJECT, BOARD_NAME, KEY]),
        writes: true,
        deletes: false,
        call: add,
    },
    Tool {
        name: "board_list",
        description: "List the project's boards, with how much is on each. Archived boards are left out unless archived says include or only.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT]);
            super::offer_archived(&mut schema);
            schema
        },
        writes: false,
        deletes: false,
        call: list,
    },
    Tool {
        name: "board_search",
        description: "Find which open project a board is in, by its key (ROAD), a card handle on it (ROAD-12) or its name. Answers every board that matches, with its project's path to pass to the other board tools. Archived boards are left out unless archived says include or only.",
        schema: |_| {
            let mut schema = fields(false, &[SOUGHT]);
            many(&mut schema, SOUGHT);
            super::offer_archived(&mut schema);
            schema
        },
        writes: false,
        deletes: false,
        call: search,
    },
    Tool {
        name: "board_search_cards",
        description: "Find text in the cards of the project's boards. Answers each matching card with its handle, its board, its column and the line it matched. Cards on archived boards are left out unless archived says include or only.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, QUERY]);
            super::offer_archived(&mut schema);
            schema
        },
        writes: false,
        deletes: false,
        call: search_cards,
    },
    Tool {
        name: "board_rename",
        description: "Rename a board, and optionally re-key it. A key another board here already has is refused.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD, BOARD_NAME_NOW]);
            schema["properties"][KEY_NOW.name] = json!({
                "type": "string",
                "description": KEY_NOW.about,
            });
            schema
        },
        writes: true,
        deletes: false,
        call: rename,
    },
    Tool {
        name: "board_read",
        description: "Read one board: its description, which is the guide for working its cards, then its columns and the cards under them by handle and first line, 50 to a column. A column holding more says so; read the rest with column and after. Read a card's whole text with board_read_card.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD]);
            super::optional(&mut schema, ONE_COLUMN, "string");
            super::optional(&mut schema, AFTER, "string");
            schema
        },
        writes: false,
        deletes: false,
        call: read,
    },
    Tool {
        name: "board_read_card",
        description: "Read one card or several by handle: each one's whole text, its column, whether it is busy, the cards it waits on, the cards waiting on it and the session that last held it, after the description of the board it is on.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARDS]);
            many(&mut schema, CARDS);
            schema
        },
        writes: false,
        deletes: false,
        call: read_card,
    },
    Tool {
        name: "board_describe",
        description: "Replace a board's description: the guide every agent working its cards reads, answered by board_read and board_read_card.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, DESCRIPTION]),
        writes: true,
        deletes: false,
        call: describe,
    },
    Tool {
        name: "board_archive",
        description: "Put a board away, or bring one back. An archived board is listed under the divider rather than gone, and its cards keep their handles.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARDS]);
            many(&mut schema, BOARDS);
            super::defaulted(&mut schema, ARCHIVED, "boolean", json!(true));
            schema
        },
        writes: true,
        deletes: false,
        call: archive,
    },
    Tool {
        name: "board_remove",
        description: "Delete a board and every card on it. This cannot be undone; archive it instead to put it away.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARDS]);
            many(&mut schema, BOARDS);
            schema
        },
        writes: true,
        deletes: true,
        call: remove,
    },
    Tool {
        name: "board_add_card",
        description: "Put a new card at the end of a column, or several in one write, and answer their handles.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD, COLUMN, TEXT]);
            many(&mut schema, TEXT);
            schema
        },
        writes: true,
        deletes: false,
        call: add_card,
    },
    Tool {
        name: "board_rewrite_card",
        description: "Replace what a card says.",
        schema: |bound| fields(bound, &[PROJECT, CARD, TEXT_NOW]),
        writes: true,
        deletes: false,
        call: rewrite_card,
    },
    Tool {
        name: "board_set_card_busy",
        description: "Tag one card or several busy while working on them, and take the tag off when the turn is over. Busy also links the calling session when available. Whether a card is done is its column: use board_move_card for that.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARDS, BUSY]);
            schema["properties"][BUSY.name]["type"] = json!("boolean");
            many(&mut schema, CARDS);
            schema
        },
        writes: true,
        deletes: false,
        call: set_card_busy,
    },
    Tool {
        name: "board_set_card_blockers",
        description: "Say which cards a card waits on.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARD, BLOCKERS]);
            many(&mut schema, BLOCKERS);
            schema
        },
        writes: true,
        deletes: false,
        call: set_card_blockers,
    },
    Tool {
        name: "board_move_card",
        description: "Carry a card or several to the end of another column, another board, or a board in another project. Moving to another board gives each a new handle. Answers the cards waiting on each one moved.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARDS]);
            for arg in [COLUMN, TO_BOARD, TO_PROJECT] {
                super::optional(&mut schema, arg, "string");
            }
            many(&mut schema, CARDS);
            schema
        },
        writes: true,
        deletes: false,
        call: move_card,
    },
    Tool {
        name: "board_remove_card",
        description: "Take a card off its board for good, or several in one write. They may be on different boards.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARDS]);
            many(&mut schema, CARDS);
            schema
        },
        writes: true,
        deletes: true,
        call: remove_card,
    },
    Tool {
        name: "board_add_column",
        description: "Add a column at the right-hand end of a board, or several in one write.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD, NAME]);
            many(&mut schema, NAME);
            schema
        },
        writes: true,
        deletes: false,
        call: add_column,
    },
    Tool {
        name: "board_rename_column",
        description: "Rename a column. Cards keep the handles they already have.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, COLUMN, NAME_NOW]),
        writes: true,
        deletes: false,
        call: rename_column,
    },
    Tool {
        name: "board_move_column",
        description: "Put a column in front of another, or at the right-hand end.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD, COLUMN]);
            schema["properties"][BEFORE_COLUMN.name] = json!({
                "type": "string",
                "description": BEFORE_COLUMN.about,
            });
            schema
        },
        writes: true,
        deletes: false,
        call: move_column,
    },
    Tool {
        name: "board_remove_column",
        description: "Drop an empty column, or several in one write. A column holding cards is refused — empty it first.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, BOARD, COLUMNS]);
            many(&mut schema, COLUMNS);
            schema
        },
        writes: true,
        deletes: false,
        call: remove_column,
    },
];

// ── the tools ────────────────────────────────────────────────────

fn add(args: Args<'_>) -> Outcome {
    let project = store(&args)?;
    let name = args.text(BOARD_NAME)?.trim();
    if name.is_empty() {
        return Err(Trouble::Refused("A board needs a name.".into()));
    }
    let key = artifact::board::key::claim(args.text(KEY)?, &project.boards(), None)?;
    let board = block_on(project.create_board(name, &key)).map_err(unwritten)?;
    let lanes = Lane::first(&board);
    let here = project.root();
    Ok(Answer::said(outline(&board, &lanes, here)).with(shape(&board, &lanes, here)))
}

/// The name, and the key with it where one was given.
///
/// Both are written in one save: a rename that took two would leave the board
/// named anew under the old key if the second never ran.
fn rename(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let name = args.text(BOARD_NAME_NOW)?.trim();
    if name.is_empty() {
        return Err(Trouble::Refused("A board needs a name.".into()));
    }
    let key = match args.maybe(KEY_NOW) {
        Some(given) => Some(artifact::board::key::claim(
            given,
            &project.boards(),
            Some(&board.id),
        )?),
        None => None,
    };
    let was = match board.name.is_empty() {
        true => board.id.clone(),
        false => board.name.clone(),
    };
    let rekeyed = key.as_ref().filter(|key| **key != board.key).map(|key| {
        let from = board.key.clone();
        (from, key.clone())
    });
    // Found under the old key: every card naming one of this board's cards
    // names it again under the new one.
    let ends: Vec<(Place, String, Vec<Counterpart>)> = match rekeyed {
        Some(_) => {
            let disk = Disk::default();
            board
                .columns
                .iter()
                .flat_map(|column| &column.cards)
                .filter_map(|card| {
                    let (place, others) = ends(&disk, project.root(), &board, card)?;
                    Some((place, card.id.clone(), others))
                })
                .collect()
        }
        None => Vec::new(),
    };
    board.name = name.to_owned();
    if let Some(key) = key {
        board.key = key;
    }
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    let edits: Vec<Edit> = ends
        .iter()
        .flat_map(|(from, card, others)| {
            let to = Place {
                key: board.key.clone(),
                ..from.clone()
            };
            blocking::moved(from, &to, &board.id, card, others)
        })
        .collect();
    write_edits(&edits)?;
    let said = match rekeyed {
        Some((from, to)) => format!("{was} is now {name}, and {from}-1 is now {to}-1"),
        None => format!("{was} is now {name}"),
    };
    let lanes = Lane::first(&board);
    Ok(Answer::said(said).with(shape(&board, &lanes, project.root())))
}

fn describe(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    board.description = args.text(DESCRIPTION)?.trim().to_owned();
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(Answer::said(match board.description.is_empty() {
        true => format!("{} has no description", spoken(&board)),
        false => format!("{} is described", spoken(&board)),
    }))
}

/// One board or a run of them, put away or brought back in one write.
fn archive(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let archived = args.boolean(ARCHIVED, true)?;
    let mut said: Vec<String> = Vec::new();
    for needle in args.list(BOARDS)? {
        let mut board = board(project, needle)?;
        board.archived = archived;
        said.push(spoken(&board));
        block_on(project.save_board(&mut board)).map_err(unwritten)?;
    }
    let what = match archived {
        true => "put away",
        false => "brought back",
    };
    Ok(Answer::said(format!("{} {what}", said.join(", "))))
}

/// One board or a run of them, off the disk for good.
///
/// Every one is found before any is removed, the way a run of columns is —
/// a half-emptied project is not what a refused call should leave behind.
fn remove(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut found = Vec::new();
    for needle in args.list(BOARDS)? {
        found.push(board(project, needle)?);
    }
    let gone: Vec<String> = found.iter().map(spoken).collect();
    let disk = Disk::default();
    let edits: Vec<Edit> = found
        .iter()
        .flat_map(|board| {
            let disk = &disk;
            board
                .columns
                .iter()
                .flat_map(|column| &column.cards)
                .filter_map(move |card| ends(disk, project.root(), board, card))
                .flat_map(|(place, others)| blocking::forget(&place, &others))
                .collect::<Vec<_>>()
        })
        .collect();
    for board in &found {
        block_on(project.remove_board(&board.id)).map_err(unwritten)?;
    }
    write_edits(&edits)?;
    Ok(Answer::said(format!("{} deleted", gone.join(", "))))
}

/// What to call a board out loud: its key and name, which is how the listing
/// says it.
fn spoken(board: &Board) -> String {
    match board.name.is_empty() {
        true => board.key.clone(),
        false => format!("{} ({})", board.key, board.name),
    }
}

fn list(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let filter = super::archived(&args)?;
    let boards: Vec<Board> = project
        .boards()
        .into_iter()
        .filter(|board| filter.keeps(board.archived))
        .collect();
    if boards.is_empty() {
        return Ok(Answer::said(match filter {
            Archived::Exclude => "this project has no boards that are not archived",
            Archived::Include => "this project has no boards",
            Archived::Only => "this project has no archived boards",
        }));
    }
    let data = boards
        .iter()
        .map(|board| {
            json!({
                "id": board.id,
                "number": board.number,
                "key": board.key,
                "name": board.name,
                "archived": board.archived,
                "columns": board.columns.len(),
                "cards": count(board),
            })
        })
        .collect::<Vec<_>>();
    Ok(Answer::said(listing(&boards)).with(json!({ "boards": data })))
}

fn search(args: Args<'_>) -> Outcome {
    let open = crate::rail::open();
    if open.is_empty() {
        return Err(Trouble::Refused("cydonia has no project open".into()));
    }
    let filter = super::archived(&args)?;
    let boards: Vec<(std::path::PathBuf, Board)> = open
        .iter()
        .filter_map(artifact::location::Location::local)
        .flat_map(|path| {
            fs::Project::new(path)
                .boards()
                .into_iter()
                .filter(|board| filter.keeps(board.archived))
                .map(move |board| (path.to_path_buf(), board))
        })
        .collect();
    let mut said = Vec::new();
    let mut data = Vec::new();
    for needle in args.list(SOUGHT)? {
        let needle = needle.trim();
        let key = match reference::parse(needle) {
            Some(Reference {
                target: Target::Card { key, .. },
                ..
            }) => key,
            _ => needle,
        };
        let found: Vec<&(std::path::PathBuf, Board)> = boards
            .iter()
            .filter(|(_, board)| same(&board.key, key) || same(&board.name, needle))
            .collect();
        if found.is_empty() {
            said.push(format!("no board {needle} in any open project"));
        }
        for (path, board) in found {
            said.push(format!(
                "{needle}: {} ({}) in {}{}",
                board.label(),
                board.key,
                path.display(),
                archived_mark(board.archived)
            ));
            data.push(json!({
                "sought": needle,
                "project": path,
                "id": board.id,
                "number": board.number,
                "key": board.key,
                "name": board.name,
                "archived": board.archived,
            }));
        }
    }
    Ok(Answer::said(said.join("\n")).with(json!({ "boards": data })))
}

/// The cards whose text holds the query, one hit per card at its first
/// matching line, in board and column order.
fn search_cards(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let filter = super::archived(&args)?;
    let query = Query::literal(args.text(QUERY)?)
        .ok_or_else(|| Trouble::Refused("query is empty".to_owned()))?;
    let boards: Vec<Board> = project
        .boards()
        .into_iter()
        .filter(|board| filter.keeps(board.archived))
        .collect();
    let (tx, rx) = mpsc::channel();
    let sources: Vec<&(dyn Searchable + Sync)> = boards
        .iter()
        .map(|board| board as &(dyn Searchable + Sync))
        .collect();
    search::run(&sources, &query, &tx);
    drop(tx);
    let mut lines: HashMap<String, String> = HashMap::new();
    for hit in rx {
        if let Block::Card(id) = hit.block {
            lines.entry(id).or_insert(hit.line);
        }
    }
    let mut said: Vec<String> = Vec::new();
    let mut hits: Vec<Value> = Vec::new();
    let mut more = false;
    'boards: for board in &boards {
        for column in &board.columns {
            for card in &column.cards {
                let Some(line) = lines.get(&card.id) else {
                    continue;
                };
                if hits.len() == HITS {
                    more = true;
                    break 'boards;
                }
                let handle = board.handle_of(card).unwrap_or_else(|| card.id.clone());
                let snippet = super::session::snippet(line);
                said.push(format!(
                    "{handle} · {} · {}{} — {snippet}",
                    spoken(board),
                    column.name,
                    archived_mark(board.archived)
                ));
                hits.push(json!({
                    "id": card.id,
                    "handle": handle,
                    "board": {
                        "id": board.id,
                        "key": board.key,
                        "name": board.name,
                        "archived": board.archived,
                    },
                    "column": { "id": column.id, "name": column.name },
                    "text": snippet,
                }));
            }
        }
    }
    if hits.is_empty() {
        return Ok(
            Answer::said(format!("no card matches {}", query.text())).with(json!({ "hits": [] }))
        );
    }
    let mut text = said.join("\n");
    if more {
        text.push_str(&format!(
            "\n… more than {HITS} cards match; narrow the query"
        ));
    }
    Ok(Answer::said(text).with(json!({ "hits": hits, "more": more })))
}

/// What follows a board or card from an archived board in an answer.
fn archived_mark(archived: bool) -> &'static str {
    match archived {
        true => " — archived",
        false => "",
    }
}

fn read(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let board = board(project, args.text(BOARD)?)?;
    let after = match args.maybe(AFTER) {
        Some(needle) => Some(card_on(&board, needle)?),
        None => None,
    };
    let column = match (args.maybe(ONE_COLUMN), &after) {
        (Some(needle), _) => Some(column(&board, needle)?),
        (None, Some(card)) => board.column_of(card).map(|column| column.id.clone()),
        (None, None) => None,
    };
    let lanes = match &column {
        Some(id) => vec![Lane::of(&board, id, after.as_deref())?],
        None => Lane::first(&board),
    };
    let here = project.root();
    Ok(Answer::said(outline(&board, &lanes, here)).with(shape(&board, &lanes, here)))
}

/// Board `id` of `project` as `board_read` answers it with no column named.
pub(crate) fn listed(project: &fs::Project, id: &str) -> Outcome {
    let board = board(project, id)?;
    let lanes = Lane::first(&board);
    let here = project.root();
    Ok(Answer::said(outline(&board, &lanes, here)).with(shape(&board, &lanes, here)))
}

fn read_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    cards(project, &args.list(CARDS)?)
}

/// The cards `needles` name in `project`, whole.
pub(crate) fn cards(project: &fs::Project, needles: &[&str]) -> Outcome {
    let mut said: Vec<String> = Vec::new();
    let mut data: Vec<Value> = Vec::new();
    let mut described: Vec<String> = Vec::new();
    let disk = Disk::default();
    for needle in needles {
        let (board, id) = locate(project, needle)?;
        if !board.description.is_empty() && !described.contains(&board.id) {
            said.push(format!(
                "{} — description\n\n{}",
                spoken(&board),
                board.description
            ));
            described.push(board.id.clone());
        }
        let card = board.card(&id).expect("located on this board");
        let column = board.column_of(&id).expect("located in a column");
        let handle = named(&board, &id);
        let mut head = format!("{handle} · {}", column.name);
        if card.busy {
            head.push_str(" · busy");
        }
        let blockers = named_cards(&disk, project.root(), &card.blocked_by);
        if !blockers.is_empty() {
            head.push_str(&format!(" · blocked by {}", blockers.join(", ")));
        }
        let waiting = named_cards(&disk, project.root(), &card.blocks);
        if !waiting.is_empty() {
            head.push_str(&format!(" · blocks {}", waiting.join(", ")));
        }
        said.push(format!("{head}\n\n{}", card.text));
        data.push(json!({
            "id": card.id,
            "handle": board.handle_of(card),
            "board": {
                "id": board.id,
                "key": board.key,
                "name": board.name,
                "description": board.description,
            },
            "column": { "id": column.id, "name": column.name },
            "text": card.text,
            "busy": card.busy,
            "blocked_by": blockers,
            "blocks": waiting,
            "session": card.session,
        }));
    }
    Ok(Answer::said(said.join("\n\n")).with(json!({ "cards": data })))
}

/// One card or a run of them, into one lane, in the order they were given.
///
/// The board is written once however many were added: it is read into memory
/// here, and a copy saved per card would each be a save of the same starting
/// point — the last would land holding only its own card.
fn add_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let column = column(&board, args.text(COLUMN)?)?;
    let name = board
        .column(&column)
        .map(|column| column.name.clone())
        .unwrap_or_default();
    let mut added: Vec<Value> = Vec::new();
    let mut handles: Vec<String> = Vec::new();
    for text in args.list(TEXT)? {
        let card = board
            .add_card(&column, text.to_owned())
            .cloned()
            .expect("the column was resolved above");
        if let Some(session) = args.session() {
            board.hold_card(&card.id, session.to_owned());
        }
        let handle = board.handle_of(&card).unwrap_or_else(|| card.id.clone());
        added.push(json!({ "id": card.id, "handle": handle }));
        handles.push(handle);
    }
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(
        Answer::said(format!("{} added to {name}", handles.join(", ")))
            .with(json!({ "cards": added })),
    )
}

fn rewrite_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let (mut board, id) = locate(project, args.text(CARD)?)?;
    let text = args.text(TEXT_NOW)?;
    let handle = named(&board, &id);
    board.rewrite_card(&id, text);
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(Answer::said(format!("{handle} now reads: {}", line(text))))
}

/// One card or a run of them, which may sit on different boards. Every card is
/// located, tagged and written in turn: a refusal on any of them is the whole
/// call refused, so a caller is never left guessing which half of a list took.
/// The cards are looked up first for that reason.
fn set_card_busy(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let busy = match args.value(BUSY) {
        Some(Value::Bool(busy)) => *busy,
        _ => {
            return Err(Trouble::Invalid(format!(
                "{} is required, as a boolean",
                BUSY.name
            )));
        }
    };
    let found: Vec<(Board, String)> = args
        .list(CARDS)?
        .into_iter()
        .map(|needle| locate(project, needle))
        .collect::<Result<_, _>>()?;
    let mut tagged: Vec<Value> = Vec::new();
    let mut handles: Vec<String> = Vec::new();
    for (mut board, ids) in by_board(found) {
        for id in ids {
            let handle = named(&board, &id);
            board.set_card_busy(&id, busy);
            if busy && let Some(session) = args.session() {
                board.hold_card(&id, session.to_owned());
            }
            tagged.push(json!({ "id": id, "handle": handle, "busy": busy }));
            handles.push(handle);
        }
        block_on(project.save_board(&mut board)).map_err(unwritten)?;
    }
    let named = handles.join(", ");
    let are = match handles.len() {
        1 => "is",
        _ => "are",
    };
    Ok(Answer::said(match busy {
        true => format!("{named} {are} busy"),
        false => format!("{named} {are} no longer busy"),
    })
    .with(json!({ "cards": tagged })))
}

/// Every blocker is located before anything is written, so a name that misses
/// refuses the whole call. Each is kept as the card's project writes it.
fn set_card_blockers(args: Args<'_>) -> Outcome {
    let here = root(&args)?;
    let project = &fs::Project::new(here);
    let (board, id) = locate(project, args.text(CARD)?)?;
    let handle = named(&board, &id);
    let mut written: Vec<String> = Vec::new();
    for needle in args.any_list(BLOCKERS)? {
        let (path, on, blocker) = blocker_in(here, needle)?;
        if path == here && blocker == id {
            return Err(Trouble::Refused(format!("{handle} cannot wait on itself")));
        }
        let card = on.card(&blocker).expect("located on this board");
        let (Some(number), false) = (card.handle, on.key.is_empty()) else {
            return Err(Trouble::Refused(format!(
                "{} has no handle to be named by",
                named(&on, &blocker)
            )));
        };
        let name = reference::project_name(&path).unwrap_or_default();
        written.push(
            Reference {
                project: Some(&name),
                target: Target::Card {
                    key: &on.key,
                    handle: number,
                },
            }
            .relative(path == here)
            .to_string(),
        );
    }
    let card = board.card(&id).expect("located on this board");
    let waiter = Place::of(here, &board, card)
        .ok_or_else(|| Trouble::Refused(format!("{handle} has no handle to be named by")))?;
    let disk = Disk::default();
    let edits = blocking::set_blockers(&disk, &waiter, &board.id, &id, &card.blocked_by, &written)
        .map_err(|missing| Trouble::Refused(format!("{missing} names no card")))?;
    write_edits(&edits)?;
    let written: Vec<String> = edits
        .iter()
        .find_map(|edit| match &edit.change {
            blocking::Change::Waits(list) if edit.card == id => Some(list.clone()),
            _ => None,
        })
        .unwrap_or_default();
    Ok(Answer::said(match written.is_empty() {
        true => format!("{handle} waits on nothing"),
        false => format!("{handle} waits on {}", written.join(", ")),
    })
    .with(json!({ "id": id, "handle": handle, "blocked_by": written })))
}

/// One card or a run of them, which may sit on different boards, moved to one
/// landing.
///
/// Every card is located and every lane resolved before any of them moves, and
/// nothing is written until all of it has taken: a refusal is the whole call
/// refused, so a caller is never left guessing which half of a list took. The
/// same rule as [`set_card_busy`], and for the same reason a board is read
/// once and written once however many of its cards were named.
fn move_card(args: Args<'_>) -> Outcome {
    let here = root(&args)?;
    let project = &fs::Project::new(here);
    let found: Vec<(Board, String)> = args
        .list(CARDS)?
        .into_iter()
        .map(|needle| locate(project, needle))
        .collect::<Result<_, _>>()?;
    let spoken = found
        .iter()
        .map(|(board, id)| named(board, id))
        .collect::<Vec<_>>()
        .join(", ");
    // Read before the move: a card carried to another board takes a new
    // handle, and every card naming it is written again to the new one.
    let disk = Disk::default();
    let before: Vec<(String, String, Place, Vec<Counterpart>)> = found
        .iter()
        .filter_map(|(board, id)| {
            let (place, others) = ends(&disk, here, board, board.card(id)?)?;
            Some((id.clone(), named(board, id), place, others))
        })
        .collect();
    let blocks: Vec<(String, Vec<String>)> = before
        .iter()
        .map(|(_, said, place, others)| (said.clone(), waiting(place, others)))
        .filter(|(_, waiting)| !waiting.is_empty())
        .collect();
    let landing = match args.maybe(TO_PROJECT) {
        Some(named) => on_the_rail(Path::new(named))?,
        None => here,
    };
    // Another board is named, or another project is — a project on its own is
    // not a destination, since a card sits on a board and not in a directory.
    let elsewhere = match (args.maybe(TO_BOARD), landing == here) {
        (None, true) => None,
        (None, false) => {
            return Err(Trouble::Refused(format!(
                "name the board in {} to move {spoken} to — it has {}",
                landing.display(),
                keys(&fs::Project::new(landing).boards())
            )));
        }
        (Some(needle), _) => Some(needle),
    };
    let sources = by_board(found);
    let Some(needle) = elsewhere else {
        return within(project, sources, args.text(COLUMN)?, &spoken)
            .map(|answer| blocking(answer, &blocks));
    };
    let destination = &fs::Project::new(landing);
    let mut to = self::board(destination, needle)?;
    // The board they are already on, where it is the one being moved to: those
    // cards are changing lanes rather than being carried, and they change them
    // on the one copy that is about to be written.
    let home = (landing == here).then(|| to.id.clone());
    if sources
        .iter()
        .all(|(board, _)| home.as_deref() == Some(&board.id))
    {
        return within(project, sources, args.text(COLUMN)?, &spoken)
            .map(|answer| blocking(answer, &blocks));
    }
    if to.columns.is_empty() {
        return Err(Trouble::Refused(format!(
            "{spoken} has nowhere to land on {} — it has no columns",
            to.label()
        )));
    }
    let lane = match args.maybe(COLUMN) {
        Some(named) => Some(column(&to, named)?),
        None => None,
    };
    let mut carried: Vec<String> = Vec::new();
    let mut landings: Vec<(String, artifact::board::Landed)> = Vec::new();
    let mut moved: Vec<Board> = Vec::new();
    for (mut board, ids) in sources {
        if home.as_deref() == Some(&board.id) {
            // Cards already on the destination, named alongside cards that are
            // not. The lane is the one asked for; without one there is no move
            // to make on this board, so they stay where they sit.
            if let Some(lane) = &lane {
                for id in &ids {
                    to.move_card(id, lane);
                    carried.push(named(&to, id));
                }
            }
            continue;
        }
        for id in &ids {
            // `carry_card` answers `None` only for a card its board does not
            // hold or a destination with no lanes, and both were ruled out
            // above — see [`locate`] and the refusal over `to.columns`.
            if let Some(landed) =
                artifact::board::carry_card(&mut board, &mut to, id, lane.as_deref())
            {
                block_on(project.carry_card_files(&board.id, id, destination, &to.id, &landed.id))
                    .map_err(unwritten)?;
                carried.push(landed.said.clone());
                landings.push((id.clone(), landed));
            }
        }
        moved.push(board);
    }
    let label = to.label().to_owned();
    for board in &mut moved {
        block_on(project.save_board(board)).map_err(unwritten)?;
    }
    block_on(destination.save_board(&mut to)).map_err(unwritten)?;
    let mut edits: Vec<Edit> = Vec::new();
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for (id, landed) in &landings {
        let Some((_, _, from, others)) = before.iter().find(|(held, ..)| held == id) else {
            continue;
        };
        let Some(handle) = to.card(&landed.id).and_then(|card| card.handle) else {
            continue;
        };
        let at = Place {
            project: landing.to_path_buf(),
            key: to.key.clone(),
            handle,
        };
        let named = waiting(&at, others);
        if !named.is_empty() {
            blocks.push((landed.said.clone(), named));
        }
        edits.extend(blocking::moved(from, &at, &to.id, &landed.id, others));
    }
    write_edits(&edits)?;
    Ok(blocking(
        Answer::said(format!(
            "{spoken} moved to {label} as {}",
            carried.join(", ")
        ))
        .with(json!({
            "cards": carried,
            "board": to.id,
            "project": landing,
        })),
        &blocks,
    ))
}

/// The cards waiting on the card at `place`, of `others` found at its
/// references, as its project writes them.
fn waiting(place: &Place, others: &[Counterpart]) -> Vec<String> {
    others
        .iter()
        .filter(|other| !other.awaited)
        .map(|other| other.place.written(&place.project))
        .collect()
}

/// A move's answer, with what waits on each card moved: `handle` and the
/// cards naming it in their blockers.
fn blocking(mut answer: Answer, blocks: &[(String, Vec<String>)]) -> Answer {
    if blocks.is_empty() {
        return answer;
    }
    for (handle, waiting) in blocks {
        answer
            .text
            .push_str(&format!("\n{handle} blocks {}", waiting.join(", ")));
    }
    let blocks: serde_json::Map<String, Value> = blocks
        .iter()
        .map(|(handle, waiting)| (handle.clone(), json!(waiting)))
        .collect();
    let mut data = answer.data.take().unwrap_or_else(|| json!({}));
    if let Some(object) = data.as_object_mut() {
        object.insert("blocks".to_owned(), Value::Object(blocks));
    }
    answer.with(data)
}

/// The same move, between the lanes of the boards the cards are already on —
/// where the column is what the move is, so it is asked for rather than
/// guessed at. A lane of each board, since a run of cards may span several.
fn within(
    project: &fs::Project,
    sources: Vec<(Board, Vec<String>)>,
    lane: &str,
    spoken: &str,
) -> Outcome {
    // Every lane found before any card moves: a name that is not on one of
    // these boards refuses the whole call rather than the half that was left.
    let mut ready: Vec<(Board, Vec<String>, String)> = Vec::new();
    for (board, ids) in sources {
        let to = column(&board, lane)?;
        ready.push((board, ids, to));
    }
    let mut name = String::new();
    let mut moved: Vec<Board> = Vec::new();
    for (mut board, ids, to) in ready {
        if let Some(column) = board.column(&to) {
            name = column.name.clone();
        }
        for id in &ids {
            board.move_card(id, &to);
        }
        moved.push(board);
    }
    for board in &mut moved {
        block_on(project.save_board(board)).map_err(unwritten)?;
    }
    Ok(Answer::said(format!("{spoken} moved to {name}")))
}

/// One card or a run of them, which may sit on different boards.
///
/// Every card is located before any is removed: a refusal on one of them is
/// the whole call refused, so a caller is never left guessing which half of a
/// list took. The same rule as [`set_card_busy`], and a board is gathered
/// once and written once however many of its cards were named.
fn remove_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let found: Vec<(Board, String)> = args
        .list(CARDS)?
        .into_iter()
        .map(|needle| locate(project, needle))
        .collect::<Result<_, _>>()?;
    let disk = Disk::default();
    let edits: Vec<Edit> = found
        .iter()
        .filter_map(|(board, id)| ends(&disk, project.root(), board, board.card(id)?))
        .flat_map(|(place, others)| blocking::forget(&place, &others))
        .collect();
    let boards = by_board(found);
    let mut gone: Vec<String> = Vec::new();
    for (mut board, ids) in boards {
        for id in &ids {
            let handle = named(&board, id);
            let card = board.remove_card(id).expect("the card was located above");
            gone.push(format!("{handle} — {}", line(&card.text)));
        }
        block_on(project.save_board(&mut board)).map_err(unwritten)?;
    }
    write_edits(&edits)?;
    Ok(Answer::said(format!("removed {}", gone.join("; "))))
}

/// One column or a run of them, at the right-hand end in the order given —
/// which is how a board is laid out in a single call rather than one per lane.
fn add_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let mut added: Vec<Value> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for name in args.list(NAME)? {
        let column = board.add_column(name);
        added.push(json!({ "id": column.id, "name": column.name }));
        names.push(column.name.clone());
    }
    let label = board.label().to_owned();
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(
        Answer::said(format!("{} added to {label}", names.join(", ")))
            .with(json!({ "columns": added })),
    )
}

fn rename_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let id = column(&board, args.text(COLUMN)?)?;
    let name = args.text(NAME_NOW)?;
    let was = board
        .column(&id)
        .map(|column| column.name.clone())
        .unwrap_or_default();
    board.rename_column(&id, name);
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(Answer::said(format!("{was} is now {name}")))
}

fn move_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let id = column(&board, args.text(COLUMN)?)?;
    let before = match args.maybe(BEFORE_COLUMN) {
        Some(named) => Some(column(&board, named)?),
        None => None,
    };
    let name = board
        .column(&id)
        .map(|column| column.name.clone())
        .unwrap_or_default();
    let anchor = before
        .as_deref()
        .and_then(|before| board.column(before))
        .map(|column| column.name.clone());
    if !board.move_column_before(&id, before.as_deref()) {
        return Err(Trouble::Refused(format!(
            "{name} is already where it is being sent"
        )));
    }
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(Answer::said(match anchor {
        Some(anchor) => format!("{name} now sits in front of {anchor}"),
        None => format!("{name} now sits at the end"),
    }))
}

/// One column or a run of them.
///
/// Every lane is found and every one of them checked empty before any is
/// dropped: a board half emptied of its lanes is not what a refused call
/// should leave behind.
fn remove_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let label = board.label().to_owned();
    let mut lanes: Vec<(String, String)> = Vec::new();
    for needle in args.list(COLUMNS)? {
        let id = column(&board, needle)?;
        let Some(column) = board.column(&id) else {
            continue;
        };
        // The refusal is the board's rule, not this tool's: a column is only
        // where work sits, so dropping one has no reading that means "and the
        // cards in it".
        if !column.cards.is_empty() {
            return Err(Trouble::Refused(format!(
                "{} still holds cards — a column is only where work sits, so move them out first",
                column.name
            )));
        }
        lanes.push((id, column.name.clone()));
    }
    let mut names: Vec<String> = Vec::new();
    for (id, name) in &lanes {
        board.remove_column(id);
        names.push(name.clone());
    }
    block_on(project.save_board(&mut board)).map_err(unwritten)?;
    Ok(Answer::said(format!(
        "{} removed from {label}",
        names.join(", ")
    )))
}

// ── addressing ───────────────────────────────────────────────────

/// The project a call is about, as the store that holds its boards.
fn store(args: &Args<'_>) -> Result<fs::Project, Trouble> {
    Ok(fs::Project::new(root(args)?))
}

/// The board a needle names: its id, its key, or its name, in that order —
/// which is least ambiguous first, since only the id is guaranteed unique.
fn board(project: &fs::Project, needle: &str) -> Result<Board, Trouble> {
    let mut boards = project.boards();
    if let Some(number) = super::number_in(project.root(), needle)? {
        return boards
            .iter()
            .position(|board| board.number == Some(number))
            .map(|at| boards.swap_remove(at))
            .ok_or_else(|| {
                Error::NotFound {
                    what: "board",
                    named: needle.to_owned(),
                }
                .into()
            });
    }
    let found = boards
        .iter()
        .position(|board| board.id == needle)
        .or_else(|| boards.iter().position(|board| same(&board.key, needle)))
        .or_else(|| boards.iter().position(|board| same(&board.name, needle)));
    match found {
        Some(at) => Ok(boards.swap_remove(at)),
        None => Err(Trouble::Refused(format!(
            "no board {needle} — this project has {}",
            keys(&boards)
        ))),
    }
}

/// The board a card is on, and the card's id on it. A handle names one card
/// across the whole project, so the board is an answer here rather than an
/// argument.
fn locate(project: &fs::Project, needle: &str) -> Result<(Board, String), Trouble> {
    let mut boards = project.boards();
    if let Some(Reference {
        project: named,
        target: Target::Card { key, handle },
    }) = reference::parse(needle)
        && let Some(at) = boards.iter().position(|board| board.keyed(key))
    {
        if let Some(name) = named
            && !reference::names(project.root(), name)
        {
            return Err(Trouble::Refused(format!(
                "{needle} is in project {name}, not this one — name that project"
            )));
        }
        let board = boards.swap_remove(at);
        let found = board.card_at(handle).map(|card| card.id.clone());
        return match found {
            Some(id) => Ok((board, id)),
            None => Err(Trouble::Refused(format!(
                "no {needle} on {}",
                board.label()
            ))),
        };
    }
    // Not a handle, so it is an id — and an id is not scoped to one board.
    for board in boards {
        if board.card(needle).is_some() {
            return Ok((board, needle.to_owned()));
        }
    }
    Err(Trouble::Refused(format!(
        "no card {needle} in this project"
    )))
}

/// The id of the card on `board` a needle names, by handle or id.
fn card_on(board: &Board, needle: &str) -> Result<String, Trouble> {
    let found = match reference::parse(needle) {
        Some(Reference {
            target: Target::Card { key, handle },
            ..
        }) if board.keyed(key) => board.card_at(handle),
        _ => board.card(needle),
    };
    found
        .map(|card| card.id.clone())
        .ok_or_else(|| Trouble::Refused(format!("no {needle} on {}", board.label())))
}

/// Located cards, one entry a board. Two cards on one board arrive as two
/// reads of it, and saving each read in turn writes back one that predates the
/// other's edit.
fn by_board(found: Vec<(Board, String)>) -> Vec<(Board, Vec<String>)> {
    let mut boards: Vec<(Board, Vec<String>)> = Vec::new();
    for (board, id) in found {
        match boards.iter_mut().find(|(held, _)| held.id == board.id) {
            Some((_, ids)) => ids.push(id),
            None => boards.push((board, vec![id])),
        }
    }
    boards
}

/// The id of the column a needle names. Nothing stops two columns sharing a
/// name, so a name that hits twice is refused rather than guessed at — the
/// caller is one `get_board` away from the ids.
fn column(board: &Board, needle: &str) -> Result<String, Trouble> {
    if let Some(found) = board.column(needle) {
        return Ok(found.id.clone());
    }
    let mut named = board
        .columns
        .iter()
        .filter(|column| same(&column.name, needle));
    let Some(first) = named.next() else {
        return Err(Trouble::Refused(format!(
            "no column {needle} on {} — it has {}",
            board.label(),
            columns(board)
        )));
    };
    match named.next() {
        None => Ok(first.id.clone()),
        Some(_) => Err(Trouble::Refused(format!(
            "{} has two columns called {needle} — name the one you mean by its id",
            board.label()
        ))),
    }
}

/// What to call a card out loud, falling back to its id on a board that has no
/// key to build a handle from.
fn named(board: &Board, id: &str) -> String {
    board
        .card(id)
        .and_then(|card| board.handle_of(card))
        .unwrap_or_else(|| id.to_owned())
}

/// Names are typed by a person or read back out of a sentence, so they match
/// however they were capitalised. Ids never reach here.
fn same(held: &str, needle: &str) -> bool {
    held.eq_ignore_ascii_case(needle)
}

// ── rendering ────────────────────────────────────────────────────

/// One page of a column's cards: at most [`PAGE`] of them, from the start or
/// after one card.
struct Lane<'a> {
    column: &'a Column,
    after: Option<&'a Card>,
    cards: &'a [Card],
    /// How many of the column's cards come after this page.
    more: usize,
}

impl<'a> Lane<'a> {
    /// The first page of every column.
    fn first(board: &'a Board) -> Vec<Self> {
        board
            .columns
            .iter()
            .map(|column| Self::page(column, 0))
            .collect()
    }

    /// The page of `column` after the card `after`, or its first.
    fn of(board: &'a Board, column: &str, after: Option<&str>) -> Result<Self, Trouble> {
        let column = board.column(column).expect("resolved on this board");
        let Some(id) = after else {
            return Ok(Self::page(column, 0));
        };
        let at = column
            .cards
            .iter()
            .position(|card| card.id == id)
            .ok_or_else(|| {
                Trouble::Refused(format!("{} is not in {}", named(board, id), column.name))
            })?;
        Ok(Self::page(column, at + 1))
    }

    fn page(column: &'a Column, start: usize) -> Self {
        let end = (start + PAGE).min(column.cards.len());
        Self {
            column,
            after: start.checked_sub(1).map(|at| &column.cards[at]),
            cards: &column.cards[start..end],
            more: column.cards.len() - end,
        }
    }
}

/// One board, as a person would have written it down.
fn outline(board: &Board, lanes: &[Lane<'_>], here: &Path) -> String {
    let disk = Disk::default();
    let mut out = match board.key.is_empty() {
        true => board.label().to_owned(),
        false => format!("{} ({})", board.label(), board.key),
    };
    out = artifact::entry::label(board.number, &out);
    out.push_str(archived_mark(board.archived));
    if !board.description.is_empty() {
        out.push_str(&format!("\n\n{}", board.description));
    }
    if board.columns.is_empty() {
        out.push_str("\n\nno columns yet");
        return out;
    }
    let width = lanes
        .iter()
        .flat_map(|lane| lane.cards)
        .filter_map(|card| board.handle_of(card))
        .map(|handle| handle.chars().count())
        .max()
        .unwrap_or(0);
    for lane in lanes {
        out.push_str(&format!("\n\n{}", lane.column.name));
        if let Some(card) = lane.after {
            out.push_str(&format!(", after {}", named(board, &card.id)));
        }
        if lane.column.cards.is_empty() {
            out.push_str("\n  (empty)");
        }
        for card in lane.cards {
            let handle = board.handle_of(card).unwrap_or_else(|| card.id.clone());
            out.push_str(&format!("\n  {handle:<width$}  {}", line(&card.text)));
            if card.busy {
                out.push_str("  [busy]");
            }
            let blockers = named_cards(&disk, here, &card.blocked_by);
            if !blockers.is_empty() {
                out.push_str(&format!("  (blocked by {})", blockers.join(", ")));
            }
        }
        if let Some(last) = lane.cards.last().filter(|_| lane.more > 0) {
            out.push_str(&format!(
                "\n  … {} more — read them with column {} and after {}",
                lane.more,
                lane.column.name,
                named(board, &last.id)
            ));
        }
    }
    out
}

/// Every board, one to a line.
fn listing(boards: &[Board]) -> String {
    boards
        .iter()
        .map(|board| {
            let held = match (board.columns.len(), count(board)) {
                (0, _) => "no columns".to_owned(),
                (columns, 0) => format!("{columns} columns, empty"),
                (columns, cards) => format!("{columns} columns, {cards} cards"),
            };
            format!(
                "{}  {held}{}",
                artifact::entry::label(board.number, &format!("{} ({})", board.label(), board.key)),
                archived_mark(board.archived)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same board as fields, for a client that renders rather than reads.
fn shape(board: &Board, lanes: &[Lane<'_>], here: &Path) -> Value {
    let disk = Disk::default();
    json!({
        "id": board.id,
        "number": board.number,
        "key": board.key,
        "name": board.name,
        "archived": board.archived,
        "description": board.description,
        "columns": lanes
            .iter()
            .map(|lane| json!({
                "id": lane.column.id,
                "name": lane.column.name,
                "count": lane.column.cards.len(),
                "after": lane.after.map(|card| named(board, &card.id)),
                "cards": lane
                    .cards
                    .iter()
                    .map(|card| json!({
                        "id": card.id,
                        "handle": board.handle_of(card),
                        "title": line(&card.text),
                        "session": card.session,
                        "busy": card.busy,
                        "blocked_by": named_cards(&disk, here, &card.blocked_by),
                    }))
                    .collect::<Vec<_>>(),
                "more": lane.more,
            }))
            .collect::<Vec<_>>(),
    })
}

/// Those of `list`, a card's references as the project at `here` writes
/// them, that still name a card.
fn named_cards(disk: &Disk, here: &Path, list: &[String]) -> Vec<String> {
    list.iter()
        .filter(|written| blocking::locate(disk, here, written).is_some())
        .cloned()
        .collect()
}

/// The open projects' boards as they are on disk, each project read once —
/// what [`blocking`] finds the other end of a card's references through.
#[derive(Default)]
struct Disk(RefCell<Vec<(PathBuf, Vec<Board>)>>);

impl Shelf for Disk {
    fn project(&self, from: &Path, name: Option<&str>) -> Option<PathBuf> {
        match name {
            None => Some(from.to_path_buf()),
            Some(name) => super::project_named(name).ok(),
        }
    }

    fn card(&self, project: &Path, key: &str, handle: u64) -> Option<Found> {
        let mut read = self.0.borrow_mut();
        let at = match read.iter().position(|(path, _)| path == project) {
            Some(at) => at,
            None => {
                read.push((project.to_path_buf(), fs::Project::new(project).boards()));
                read.len() - 1
            }
        };
        let board = read[at].1.iter().find(|board| board.keyed(key))?;
        Some(Found {
            board: board.id.clone(),
            key: board.key.clone(),
            card: board.card_at(handle)?.id.clone(),
        })
    }
}

/// Where `card` of `board`, a board of the project at `here`, is, and the
/// cards its lists name — read before it moves or goes. `None` for a card
/// nothing can name.
fn ends(disk: &Disk, here: &Path, board: &Board, card: &Card) -> Option<(Place, Vec<Counterpart>)> {
    let place = Place::of(here, board, card)?;
    let others = blocking::counterparts(disk, here, card);
    Some((place, others))
}

/// Lay `edits` on the boards they land on, each read as it is now and
/// written once.
fn write_edits(edits: &[Edit]) -> Result<(), Trouble> {
    for (path, id) in blocking::boards(edits) {
        let project = fs::Project::new(&path);
        let Some(mut board) = project.board(&id) else {
            continue;
        };
        if blocking::apply(edits, &path, &mut board) {
            block_on(project.save_board(&mut board)).map_err(unwritten)?;
        }
    }
    Ok(())
}

/// The project, board and card `needle` names: a card reference as written in
/// the project at `here` — `DEV-12`, or `foo#DEV-12` in another open project —
/// or a card's id on a board of `here`.
fn blocker_in(here: &Path, needle: &str) -> Result<(PathBuf, Board, String), Trouble> {
    let path = match reference::parse(needle).and_then(|reference| reference.project) {
        Some(name) => super::project_named(name)?,
        None => here.to_path_buf(),
    };
    let (board, id) = locate(&fs::Project::new(&path), needle)?;
    Ok((path, board, id))
}

/// A card's first line.
fn line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn count(board: &Board) -> usize {
    board.columns.iter().map(|column| column.cards.len()).sum()
}

fn keys(boards: &[Board]) -> String {
    super::listing(
        boards
            .iter()
            .map(|board| format!("{} ({})", board.key, board.label())),
    )
}

fn columns(board: &Board) -> String {
    match board.columns.is_empty() {
        true => "no columns at all".to_owned(),
        false => board
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// A write the backend refused, as the answer the agent reads.
fn unwritten(error: impl Into<anyhow::Error>) -> Trouble {
    Error::failed("the board", Act::Written, error).into()
}
