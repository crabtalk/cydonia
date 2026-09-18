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
    tools::{PROJECT, fields, on_the_rail, root},
};
use artifact::{
    board::Board,
    project::{Project, fs},
};
use serde_json::{Value, json};
use std::path::Path;

const BOARD: Arg = Arg {
    name: "board",
    about: "The board: its project reference (#12), key (ROAD), name, or storage id.",
};
const CARD: Arg = Arg {
    name: "card",
    about: "The card: its handle (ROAD-12), or its id.",
};
const COLUMN: Arg = Arg {
    name: "column",
    about: "The column: its name, or its id.",
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

const BEFORE_COLUMN: Arg = Arg {
    name: "before",
    about: "The column to put it in front of, by name or id. Left out, it goes to the right-hand end.",
};

/// The two `text` arguments and the two `name` ones carry the same key and a
/// different line: what a card says when it is made is not what it should say
/// now. A const each, so the tool that takes one names the one it takes.
const TEXT: Arg = Arg {
    name: "text",
    about: "What the card says.",
};
const TEXT_NOW: Arg = Arg {
    name: "text",
    about: "What the card should say now.",
};
const NAME: Arg = Arg {
    name: "name",
    about: "What the column is called.",
};
const NAME_NOW: Arg = Arg {
    name: "name",
    about: "What the column should be called now.",
};

const BOARD_NAME: Arg = Arg {
    name: "name",
    about: "What the new board is called.",
};
const KEY: Arg = Arg {
    name: "key",
    about: "A unique board key for card handles, such as ROAD. Normalized to uppercase letters and digits.",
};

pub static TOOLS: [Tool; 11] = [
    Tool {
        name: "board_add",
        description: "Create a board with a name and unique key. Returns its id, project number, key, and columns. Use board_add_column to add columns.",
        schema: |bound| fields(bound, &[PROJECT, BOARD_NAME, KEY]),
        writes: true,
        call: add,
    },
    Tool {
        name: "board_list",
        description: "List the project's boards, with how much is on each.",
        schema: |bound| fields(bound, &[PROJECT]),
        writes: false,
        call: list,
    },
    Tool {
        name: "board_read",
        description: "Read one board: its columns, and the cards under them by handle.",
        schema: |bound| fields(bound, &[PROJECT, BOARD]),
        writes: false,
        call: read,
    },
    Tool {
        name: "board_add_card",
        description: "Put a new card at the end of a column, and answer its handle.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, COLUMN, TEXT]),
        writes: true,
        call: add_card,
    },
    Tool {
        name: "board_rewrite_card",
        description: "Replace what a card says.",
        schema: |bound| fields(bound, &[PROJECT, CARD, TEXT_NOW]),
        writes: true,
        call: rewrite_card,
    },
    Tool {
        name: "board_move_card",
        description: "Carry a card to the end of another column, another board, or a board in another project. Moving to another board gives it a new handle and clears the session it was dispatched to.",
        schema: |bound| {
            let mut schema = fields(bound, &[PROJECT, CARD, COLUMN, TO_BOARD, TO_PROJECT]);
            schema["required"] = json!([PROJECT.name, CARD.name]
                .iter()
                .filter(|name| !bound || **name != PROJECT.name)
                .collect::<Vec<_>>());
            schema
        },
        writes: true,
        call: move_card,
    },
    Tool {
        name: "board_remove_card",
        description: "Take a card off its board for good.",
        schema: |bound| fields(bound, &[PROJECT, CARD]),
        writes: true,
        call: remove_card,
    },
    Tool {
        name: "board_add_column",
        description: "Add a column at the right-hand end of a board.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, NAME]),
        writes: true,
        call: add_column,
    },
    Tool {
        name: "board_rename_column",
        description: "Rename a column. Cards keep the handles they already have.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, COLUMN, NAME_NOW]),
        writes: true,
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
        call: move_column,
    },
    Tool {
        name: "board_remove_column",
        description: "Drop an empty column. A column holding cards is refused — empty it first.",
        schema: |bound| fields(bound, &[PROJECT, BOARD, COLUMN]),
        writes: true,
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
    let key = artifact::board::key::normalize(args.text(KEY)?)
        .ok_or_else(|| Trouble::Refused("A key needs at least one letter or digit.".into()))?;
    if project.boards().iter().any(|board| board.key == key) {
        return Err(Trouble::Refused(format!(
            "{key} is another board's key here."
        )));
    }
    let board = project
        .create_board(name, &key)
        .ok_or_else(|| Trouble::Refused("The board could not be written.".into()))?;
    Ok(Answer::said(outline(&board)).with(shape(&board)))
}

fn list(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let boards = project.boards();
    if boards.is_empty() {
        return Ok(Answer::said("this project has no boards"));
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

fn read(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let board = board(project, args.text(BOARD)?)?;
    Ok(Answer::said(outline(&board)).with(shape(&board)))
}

fn add_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let column = column(&board, args.text(COLUMN)?)?;
    let text = args.text(TEXT)?.to_owned();
    let name = board.column(&column).map(|column| column.name.clone());
    let card = board
        .add_card(&column, text)
        .cloned()
        .expect("the column was resolved a line ago");
    let handle = board.handle_of(&card).unwrap_or_else(|| card.id.clone());
    project.save_board(&mut board);
    Ok(
        Answer::said(format!("{handle} added to {}", name.unwrap_or_default()))
            .with(json!({ "id": card.id, "handle": handle })),
    )
}

fn rewrite_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let (mut board, id) = locate(project, args.text(CARD)?)?;
    let text = args.text(TEXT_NOW)?;
    let handle = named(&board, &id);
    board.rewrite_card(&id, text);
    project.save_board(&mut board);
    Ok(Answer::said(format!("{handle} now reads: {}", line(text))))
}

fn move_card(args: Args<'_>) -> Outcome {
    let here = root(&args)?;
    let project = &fs::Project::new(here);
    let (mut board, id) = locate(project, args.text(CARD)?)?;
    let handle = named(&board, &id);
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
                "name the board in {} to move {handle} to — it has {}",
                landing.display(),
                keys(&fs::Project::new(landing).boards())
            )));
        }
        (Some(needle), _) => Some(needle),
    };
    let Some(needle) = elsewhere else {
        return within(project, board, &id, &handle, args.text(COLUMN)?);
    };
    let destination = &fs::Project::new(landing);
    let mut to = self::board(destination, needle)?;
    if to.id == board.id && landing == here {
        return within(project, board, &id, &handle, args.text(COLUMN)?);
    }
    let lane = match args.maybe(COLUMN) {
        Some(named) => Some(column(&to, named)?),
        None => None,
    };
    let landed = artifact::board::carry_card(&mut board, &mut to, &id, lane.as_deref())
        .ok_or_else(|| {
            Trouble::Refused(format!(
                "{handle} has nowhere to land on {} — it has no columns",
                to.label()
            ))
        })?;
    let label = to.label().to_owned();
    destination.save_board(&mut to);
    project.save_board(&mut board);
    Ok(
        Answer::said(format!("{handle} moved to {label} as {landed}")).with(json!({
            "card": landed,
            "board": to.id,
            "project": landing,
        })),
    )
}

/// The same move, between two lanes of one board — where the column is what
/// the move is, so it is asked for rather than guessed at.
fn within(
    project: &fs::Project,
    mut board: Board,
    id: &str,
    handle: &str,
    named: &str,
) -> Outcome {
    let to = column(&board, named)?;
    let name = board.column(&to).map(|column| column.name.clone());
    if !board.move_card(id, &to) {
        return Err(Trouble::Refused(format!("{handle} would not move")));
    }
    project.save_board(&mut board);
    Ok(Answer::said(format!(
        "{handle} moved to {}",
        name.unwrap_or_default()
    )))
}

fn remove_card(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let (mut board, id) = locate(project, args.text(CARD)?)?;
    let handle = named(&board, &id);
    let card = board
        .remove_card(&id)
        .expect("the card was located a line ago");
    project.save_board(&mut board);
    Ok(Answer::said(format!(
        "{handle} removed — {}",
        line(&card.text)
    )))
}

fn add_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let name = args.text(NAME)?;
    let id = board.add_column(name).id.clone();
    let label = board.label().to_owned();
    project.save_board(&mut board);
    Ok(Answer::said(format!("{name} added to {label}")).with(json!({ "id": id })))
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
    project.save_board(&mut board);
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
    project.save_board(&mut board);
    Ok(Answer::said(match anchor {
        Some(anchor) => format!("{name} now sits in front of {anchor}"),
        None => format!("{name} now sits at the end"),
    }))
}

fn remove_column(args: Args<'_>) -> Outcome {
    let project = &store(&args)?;
    let mut board = board(project, args.text(BOARD)?)?;
    let id = column(&board, args.text(COLUMN)?)?;
    let name = board
        .column(&id)
        .map(|column| column.name.clone())
        .unwrap_or_default();
    let label = board.label().to_owned();
    // The refusal is the board's, not this tool's: a column is only where work
    // sits, so dropping one has no reading that means "and the cards in it".
    if !board.remove_column(&id) {
        return Err(Trouble::Refused(format!(
            "{name} still holds cards — a column is only where work sits, so move them out first"
        )));
    }
    project.save_board(&mut board);
    Ok(Answer::said(format!("{name} removed from {label}")))
}

// ── addressing ───────────────────────────────────────────────────

/// The project a call is about, as the store that holds its boards.
fn store(args: &Args<'_>) -> Result<fs::Project, Trouble> {
    Ok(fs::Project::new(root(args)?))
}

/// The board a needle names: its id, its key, or its name, in that order —
/// which is least ambiguous first, since only the id is guaranteed unique.
fn board(project: &impl Project, needle: &str) -> Result<Board, Trouble> {
    let mut boards = project.boards();
    if let Some(number) = artifact::entry::reference(needle) {
        return boards
            .iter()
            .position(|board| board.number == Some(number))
            .map(|at| boards.swap_remove(at))
            .ok_or_else(|| Trouble::Refused(format!("no board {needle} in this project")));
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
fn locate(project: &impl Project, needle: &str) -> Result<(Board, String), Trouble> {
    let mut boards = project.boards();
    // `ROA2-5` is card 5 on board ROA2, so the split is the *last* dash — a
    // key may carry a digit, and boards.md settled which side it falls on.
    if let Some((key, number)) = needle.rsplit_once('-')
        && let Ok(handle) = number.parse::<u64>()
        && let Some(at) = boards.iter().position(|board| same(&board.key, key))
    {
        let board = boards.swap_remove(at);
        let found = board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .find(|card| card.handle == Some(handle))
            .map(|card| card.id.clone());
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

/// One board, as a person would have written it down.
fn outline(board: &Board) -> String {
    let mut out = match board.key.is_empty() {
        true => board.label().to_owned(),
        false => format!("{} ({})", board.label(), board.key),
    };
    out = artifact::entry::label(board.number, &out);
    if board.archived {
        out.push_str(" — archived");
    }
    if board.columns.is_empty() {
        out.push_str("\n\nno columns yet");
        return out;
    }
    let width = board
        .columns
        .iter()
        .flat_map(|column| &column.cards)
        .filter_map(|card| board.handle_of(card))
        .map(|handle| handle.chars().count())
        .max()
        .unwrap_or(0);
    for column in &board.columns {
        out.push_str(&format!("\n\n{}", column.name));
        if column.cards.is_empty() {
            out.push_str("\n  (empty)");
        }
        for card in &column.cards {
            let handle = board.handle_of(card).unwrap_or_else(|| card.id.clone());
            out.push_str(&format!("\n  {handle:<width$}  {}", line(&card.text)));
            // What the card was handed to, which is what ▶ and 💬 are drawn
            // off. Whether that session is *running* is the app's to know.
            if card.session.is_some() {
                out.push_str("  (dispatched)");
            }
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
            let archived = match board.archived {
                true => " — archived",
                false => "",
            };
            format!(
                "{}  {held}{archived}",
                artifact::entry::label(board.number, &format!("{} ({})", board.label(), board.key))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same board as fields, for a client that renders rather than reads.
fn shape(board: &Board) -> Value {
    json!({
        "id": board.id,
        "number": board.number,
        "key": board.key,
        "name": board.name,
        "archived": board.archived,
        "columns": board
            .columns
            .iter()
            .map(|column| json!({
                "id": column.id,
                "name": column.name,
                "cards": column
                    .cards
                    .iter()
                    .map(|card| json!({
                        "id": card.id,
                        "handle": board.handle_of(card),
                        "text": card.text,
                        "session": card.session,
                    }))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// A card is one line in an outline however many it has. The whole of it is in
/// `structuredContent`, for the caller that wants the rest.
fn line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn count(board: &Board) -> usize {
    board.columns.iter().map(|column| column.cards.len()).sum()
}

fn keys(boards: &[Board]) -> String {
    match boards.is_empty() {
        true => "none at all".to_owned(),
        false => boards
            .iter()
            .map(|board| format!("{} ({})", board.key, board.label()))
            .collect::<Vec<_>>()
            .join(", "),
    }
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
