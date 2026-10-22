//! The transcript's list rows: one per block a turn is drawn as, so an update
//! to one item lays out that item's row and leaves the rest of its turn alone.

use crate::{
    model::session::ChatSession,
    view::component::transcript::{State, Turn, interim, turns},
};
use artifact::session::chat::ChatItem;
use std::ops::Range;

/// One row of the transcript's list, keyed by the item it starts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    /// The question a turn opens on.
    Asked(usize),
    /// An item drawn where it falls in its turn: prose, a notice, the agent
    /// process.
    Said(usize),
    /// The header of the run of work starting at this item.
    Work(usize),
    /// One item of an open run of work.
    Step(usize),
    /// Under the last turn: the queue, and the room the composer floats over.
    Tail,
}

/// The rows a transcript is listed as, and the turn each one is in.
#[derive(Default)]
pub(super) struct Rows {
    pub(super) rows: Vec<Row>,
    /// The items each row draws: a run's whole span for its header, nothing
    /// for the tail.
    pub(super) spans: Vec<Range<usize>>,
    /// The turn each row is in, from 0. The tail's is the count of turns.
    pub(super) turn: Vec<usize>,
    /// The first row of each turn.
    pub(super) first: Vec<usize>,
    pub(super) turns: Vec<Turn>,
    /// Whether the last turn is drawn as the one in flight.
    pub(super) running: bool,
}

impl Rows {
    /// The rows `chat` is listed as, with the runs of work `state` holds open
    /// listed step by step. `running` draws the last turn as the one in
    /// flight while the session streams.
    pub(super) fn of(chat: &ChatSession, state: &State, running: bool) -> Self {
        let mut rows = Self {
            turns: turns(&chat.items),
            running: running && chat.streaming,
            ..Self::default()
        };
        let count = rows.turns.len();
        for at in 0..count {
            let range = rows.turns[at].range.clone();
            rows.first.push(rows.rows.len());
            let asked = matches!(chat.items[range.start], ChatItem::User(_));
            if asked {
                rows.push(Row::Asked(range.start), range.start..range.start + 1, at);
            }
            let body = (range.start + usize::from(asked))..range.end;
            let running = rows.running && at + 1 == count;
            let mut start = body.start;
            for run in chat.items[body].chunk_by(|a, b| interim(a) == interim(b)) {
                let span = start..start + run.len();
                start = span.end;
                if !interim(&run[0]) {
                    for ix in span {
                        rows.push(Row::Said(ix), ix..ix + 1, at);
                    }
                    continue;
                }
                let live = running && span.end == range.end;
                let open = state
                    .work
                    .get(&span.start)
                    .copied()
                    .unwrap_or_default()
                    .get(live);
                rows.push(Row::Work(span.start), span.clone(), at);
                if open {
                    for ix in span {
                        rows.push(Row::Step(ix), ix..ix + 1, at);
                    }
                }
            }
        }
        rows.push(Row::Tail, chat.items.len()..chat.items.len(), count);
        rows
    }

    fn push(&mut self, row: Row, span: Range<usize>, turn: usize) {
        self.rows.push(row);
        self.spans.push(span);
        self.turn.push(turn);
    }

    /// The row drawing the prose of item `item`: a question or something said.
    pub(super) fn row_of(&self, item: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Asked(ix) | Row::Said(ix) if *ix == item))
    }

    /// Whether `at` is the last row of its turn.
    pub(super) fn ends_turn(&self, at: usize) -> bool {
        self.turn.get(at + 1) != self.turn.get(at)
    }

    /// The turns rows `rows` are in.
    pub(super) fn turns_of(&self, rows: Range<usize>) -> Range<usize> {
        let count = self.turns.len();
        let turn = |at: usize| self.turn.get(at).copied().unwrap_or(count).min(count);
        match rows.is_empty() {
            true => turn(rows.start)..turn(rows.start),
            false => turn(rows.start)..(turn(rows.end - 1) + 1).min(count),
        }
    }

    /// The items of the turns within two of those rows `rows` are in: what a
    /// short scroll comes back to, kept laid out and selected.
    pub(super) fn items_near(&self, rows: Range<usize>) -> Range<usize> {
        let turns = self.turns_of(rows);
        let none = self.spans.last().map_or(0, |span| span.start);
        let start = turns.start.saturating_sub(2);
        let end = (turns.end + 2).min(self.turns.len());
        self.turns.get(start).map_or(none, |turn| turn.range.start)
            ..self
                .turns
                .get(end.saturating_sub(1))
                .map_or(none, |turn| turn.range.end)
    }
}
