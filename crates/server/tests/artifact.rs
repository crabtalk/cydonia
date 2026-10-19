//! The `artifact` service, over [`common::Served`]: a card's files carried
//! between two projects on the server.

mod common;

use std::path::Path;

use artifact::project::{
    Project as _, fs,
    service::{Answer, Artifact, Call, Request},
};
use common::{Scratch, Served};
use futures::future::BoxFuture;

const FILE: &str = "notes.md";

/// Sends `call` on the project at `root` now.
fn send(served: &Served, root: &Path, call: Call) -> BoxFuture<'static, Answer> {
    let answer = served.client.service::<Artifact>().send(&Request {
        root: root.to_owned(),
        call,
    });
    Box::pin(async move {
        match answer.await.unwrap() {
            Ok(answer) => answer,
            Err(refusal) => panic!("refused: {refusal:?}"),
        }
    })
}

/// A board in the project at `root` whose card `c` has a file holding
/// `text`.
fn board_with_files(root: &Path, text: &str) -> String {
    let project = fs::Project::new(root);
    let board = futures::executor::block_on(project.create_board("Board", "BRD")).unwrap();
    let dir = project.card_dir(&board.id, "c");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE), text).unwrap();
    board.id
}

fn carry(board: &str, to: &Path, to_board: &str) -> Call {
    Call::CarryCardFiles {
        board: board.to_owned(),
        card: "c".to_owned(),
        to: to.to_owned(),
        to_board: to_board.to_owned(),
        to_card: "c".to_owned(),
    }
}

#[test]
fn a_write_on_the_landing_project_waits_for_a_carry_into_it() {
    let from = Scratch::new("carry-from");
    let to = Scratch::new("carry-to");
    let board = board_with_files(from.path(), "carried");
    let landing = board_with_files(to.path(), "landing");
    let served = Served::new();

    let carried = send(&served, from.path(), carry(&board, to.path(), &landing));
    let removed = send(&served, to.path(), Call::RemoveBoard(landing.clone()));
    assert!(matches!(served.runtime.block_on(carried), Answer::Done));
    assert!(matches!(served.runtime.block_on(removed), Answer::Done));
    assert!(!fs::Project::new(to.path()).board_dir(&landing).exists());
}

#[test]
fn carries_both_ways_at_once_both_land() {
    let left = Scratch::new("carry-left");
    let right = Scratch::new("carry-right");
    let left_board = board_with_files(left.path(), "left");
    let right_board = board_with_files(right.path(), "right");
    let served = Served::new();

    let rightward = send(
        &served,
        left.path(),
        carry(&left_board, right.path(), &right_board),
    );
    let leftward = send(
        &served,
        right.path(),
        carry(&right_board, left.path(), &left_board),
    );
    assert!(matches!(served.runtime.block_on(rightward), Answer::Done));
    assert!(matches!(served.runtime.block_on(leftward), Answer::Done));
    for (root, board) in [(left.path(), &left_board), (right.path(), &right_board)] {
        let file = fs::Project::new(root).card_dir(board, "c").join(FILE);
        assert_eq!(std::fs::read_to_string(file).unwrap(), "left");
    }
}
