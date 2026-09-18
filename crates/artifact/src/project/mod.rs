//! A project, as whatever holds it: the boards, sessions and articles in one,
//! and the reads and writes over them.
//!
//! [`Project`] is what a backend answers, and [`fs`] is the one cydonia ships
//! — files under the project's own `.cydonia/`. The shapes themselves name no
//! file and open none, which is what lets a board be the same board whichever
//! backend handed it over.
//!
//! Writes answer nothing. A board that cannot be written is not worth failing
//! a click over, and the person who can see the file has more to go on than
//! the caller would. A backend that can fail in ways worth reporting is the
//! reason this would grow an error type.

pub mod fs;

use crate::{board::Board, session::record::Record};

pub trait Project {
    /// This project's boards, most recently written first.
    fn boards(&self) -> Vec<Board>;

    /// Mint a board and file it, called and keyed as the caller has them.
    /// An empty key is derived from the name, clear of the keys the project's
    /// other boards hold — see [`crate::board::key`]. Nothing for a project
    /// that cannot be written to at all.
    fn create_board(&self, name: &str, key: &str) -> Option<Board>;

    /// Write a board back, and take the time it was written at — the key the
    /// sidebar orders on, which only the backend knows.
    fn save_board(&self, board: &mut Board);

    fn remove_board(&self, id: &str);

    /// Every session filed here, most recently updated first.
    fn sessions(&self) -> Vec<Record>;

    /// Mint the id a session is filed under from here on. Called on its first
    /// write and not before: opening a project must not put anything in it.
    fn create_session(&self) -> Option<String>;

    fn save_session(&self, record: &Record);

    fn remove_session(&self, id: &str);
}
