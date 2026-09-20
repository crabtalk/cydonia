//! Moving an article to another project, and a card to another board.

mod common;

use common::{Rail, Scratch, refused, said};
use serde_json::json;

/// An article moves, its document with it, and it answers under the number the
/// project it landed in gave it.
#[test]
fn an_article_moves_to_another_open_project() {
    let here = Scratch::new("move-article-here");
    let there = Scratch::new("move-article-there");
    let server = here.server();
    Rail::also(there.path());
    said(server.call(
        "article_add",
        json!({"title": "Findings", "text": "what A knows"}),
        Some(here.path()),
    ));

    let moved = said(server.call(
        "article_move",
        json!({"article": "Findings", "to_project": there.path()}),
        Some(here.path()),
    ));

    assert!(moved.contains("moved to"), "{moved}");
    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": "Findings"}),
            Some(there.path())
        )),
        "what A knows"
    );
    assert!(
        said(server.call("article_list", json!({}), Some(here.path()))).contains("no articles")
    );
}

/// A project cydonia does not have open is not somewhere to move an article.
#[test]
fn an_article_does_not_move_to_a_project_that_is_shut() {
    let here = Scratch::new("move-shut-here");
    let shut = Scratch::new("move-shut-there");
    let server = here.server();
    said(server.call(
        "article_add",
        json!({"title": "Findings", "text": "x"}),
        Some(here.path()),
    ));

    let why = refused(server.call(
        "article_move",
        json!({"article": "Findings", "to_project": shut.path()}),
        Some(here.path()),
    ));

    assert!(why.contains("does not have"), "{why}");
    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": "Findings"}),
            Some(here.path())
        )),
        "x"
    );
}

/// Moving an article to where it already is says so rather than shuffling it.
#[test]
fn an_article_does_not_move_to_its_own_project() {
    let here = Scratch::new("move-same");
    let server = here.server();
    said(server.call(
        "article_add",
        json!({"title": "Findings", "text": "x"}),
        Some(here.path()),
    ));

    let why = refused(server.call(
        "article_move",
        json!({"article": "Findings", "to_project": here.path()}),
        Some(here.path()),
    ));

    assert!(why.contains("already in"), "{why}");
}

// ── cards ────────────────────────────────────────────────────────

/// Put a board with two lanes in a project, and answer its key.
fn board(scratch: &Scratch, server: &cydonia_mcp::Server, name: &str, key: &str) {
    said(server.call(
        "board_add",
        json!({"project": scratch.path(), "name": name, "key": key}),
        None,
    ));
    for column in ["TODO", "DOING"] {
        said(server.call(
            "board_add_column",
            json!({"project": scratch.path(), "board": key, "name": column}),
            None,
        ));
    }
}

/// The old call still means what it did: no board named is a move between the
/// lanes of the one the card is on.
#[test]
fn a_card_still_moves_between_lanes_of_its_own_board() {
    let here = Scratch::new("move-card-lanes");
    let server = here.server();
    board(&here, &server, "Roadmap", "ROAD");
    said(server.call(
        "board_add_card",
        json!({"board": "ROAD", "column": "TODO", "text": "ship it"}),
        Some(here.path()),
    ));

    let moved = said(server.call(
        "board_move_card",
        json!({"card": "ROAD-1", "column": "DOING"}),
        Some(here.path()),
    ));

    assert_eq!(moved, "ROAD-1 moved to DOING");
}

/// A card carried to a board in another project takes that board's handle and
/// lands in the lane of the name it came out of.
#[test]
fn a_card_moves_to_a_board_in_another_project() {
    let here = Scratch::new("move-card-here");
    let there = Scratch::new("move-card-there");
    let server = here.server();
    Rail::also(there.path());
    board(&here, &server, "Roadmap", "ROAD");
    board(&there, &server, "Plans", "PLAN");
    said(server.call(
        "board_add_card",
        json!({"board": "ROAD", "column": "DOING", "text": "ship it"}),
        Some(here.path()),
    ));

    let moved = said(server.call(
        "board_move_card",
        json!({"card": "ROAD-1", "to_board": "PLAN", "to_project": there.path()}),
        Some(here.path()),
    ));

    assert!(moved.contains("PLAN-1"), "{moved}");
    let landed = said(server.call(
        "board_read",
        json!({"project": there.path(), "board": "PLAN"}),
        None,
    ));
    assert!(landed.contains("ship it"), "{landed}");
    assert!(landed.contains("PLAN-1"), "{landed}");
    // And it is gone from where it was.
    let left = said(server.call("board_read", json!({"board": "ROAD"}), Some(here.path())));
    assert!(!left.contains("ship it"), "{left}");
}

/// A project on its own is not a destination — a card sits on a board.
#[test]
fn a_project_without_a_board_is_not_somewhere_to_move_a_card() {
    let here = Scratch::new("move-card-noboard-here");
    let there = Scratch::new("move-card-noboard-there");
    let server = here.server();
    Rail::also(there.path());
    board(&here, &server, "Roadmap", "ROAD");
    board(&there, &server, "Plans", "PLAN");
    said(server.call(
        "board_add_card",
        json!({"board": "ROAD", "column": "TODO", "text": "ship it"}),
        Some(here.path()),
    ));

    let why = refused(server.call(
        "board_move_card",
        json!({"card": "ROAD-1", "to_project": there.path()}),
        Some(here.path()),
    ));

    assert!(why.contains("name the board"), "{why}");
    assert!(why.contains("PLAN"), "{why}");
}

/// A run of cards moves in one call, and the board they came off is written
/// once — a second copy saved over the first would put back every card but the
/// last.
#[test]
fn several_cards_move_between_lanes_in_one_call() {
    let here = Scratch::new("move-cards-batch");
    let server = here.server();
    board(&here, &server, "Roadmap", "ROAD");
    for text in ["ship it", "then this", "and this"] {
        said(server.call(
            "board_add_card",
            json!({"board": "ROAD", "column": "TODO", "text": text}),
            Some(here.path()),
        ));
    }

    let moved = said(server.call(
        "board_move_card",
        json!({"card": ["ROAD-1", "ROAD-3"], "column": "DOING"}),
        Some(here.path()),
    ));

    assert_eq!(moved, "ROAD-1, ROAD-3 moved to DOING");
    let read = said(server.call("board_read", json!({"board": "ROAD"}), Some(here.path())));
    let doing = read.split("DOING").nth(1).unwrap_or_default();
    assert!(doing.contains("ship it"), "{read}");
    assert!(doing.contains("and this"), "{read}");
    // And the one that was not named stayed where it was.
    let todo = read.split("DOING").next().unwrap_or_default();
    assert!(todo.contains("then this"), "{read}");
}

/// A run of cards carried to another project takes that board's handles, and
/// the whole run is refused where one of them cannot be found.
#[test]
fn several_cards_move_to_another_board_or_none_do() {
    let here = Scratch::new("move-cards-batch-here");
    let there = Scratch::new("move-cards-batch-there");
    let server = here.server();
    Rail::also(there.path());
    board(&here, &server, "Roadmap", "ROAD");
    board(&there, &server, "Plans", "PLAN");
    for text in ["ship it", "then this"] {
        said(server.call(
            "board_add_card",
            json!({"board": "ROAD", "column": "TODO", "text": text}),
            Some(here.path()),
        ));
    }

    // One of the two is not a card, so neither moves.
    let why = refused(server.call(
        "board_move_card",
        json!({"card": ["ROAD-1", "ROAD-9"], "to_board": "PLAN", "to_project": there.path()}),
        Some(here.path()),
    ));
    assert!(why.contains("ROAD-9"), "{why}");
    let left = said(server.call("board_read", json!({"board": "ROAD"}), Some(here.path())));
    assert!(left.contains("ship it"), "{left}");

    let moved = said(server.call(
        "board_move_card",
        json!({"card": ["ROAD-1", "ROAD-2"], "to_board": "PLAN", "to_project": there.path()}),
        Some(here.path()),
    ));

    assert!(moved.contains("PLAN-1"), "{moved}");
    assert!(moved.contains("PLAN-2"), "{moved}");
    let landed = said(server.call(
        "board_read",
        json!({"project": there.path(), "board": "PLAN"}),
        None,
    ));
    assert!(landed.contains("ship it"), "{landed}");
    assert!(landed.contains("then this"), "{landed}");
    let left = said(server.call("board_read", json!({"board": "ROAD"}), Some(here.path())));
    assert!(!left.contains("ship it"), "{left}");
    assert!(!left.contains("then this"), "{left}");
}

/// Several articles move in one call, each with its own number in the project
/// they land in. A title that names nothing refuses the whole call, so none of
/// them travels on a list with a typo in it.
#[test]
fn several_articles_move_to_another_project() {
    let here = Scratch::new("move-articles-here");
    let there = Scratch::new("move-articles-there");
    let server = here.server();
    Rail::also(there.path());
    for (title, text) in [("Findings", "what A knows"), ("Notes", "what B knows")] {
        said(server.call(
            "article_add",
            json!({"title": title, "text": text}),
            Some(here.path()),
        ));
    }

    // One of the two is not an article, so neither moves.
    let why = refused(server.call(
        "article_move",
        json!({"article": ["Findings", "Nothing"], "to_project": there.path()}),
        Some(here.path()),
    ));
    assert!(why.contains("Nothing"), "{why}");
    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": "Findings"}),
            Some(here.path())
        )),
        "what A knows"
    );

    let moved = said(server.call(
        "article_move",
        json!({"article": ["Findings", "Notes"], "to_project": there.path()}),
        Some(here.path()),
    ));

    assert!(moved.contains("Findings"), "{moved}");
    assert!(moved.contains("Notes"), "{moved}");
    assert_eq!(
        said(server.call(
            "article_read",
            json!({"article": "Notes"}),
            Some(there.path())
        )),
        "what B knows"
    );
    assert!(
        said(server.call("article_list", json!({}), Some(here.path()))).contains("no articles")
    );
}
