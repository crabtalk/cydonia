//! What `@` lists for a query: references by number and project, board keys,
//! then titles.

use super::*;
use crate::{model::workspace::Showing, view::sidebar::Row};

fn entry(project: &str, active: bool, number: u64, title: &str, key: Option<&str>) -> Linkable {
    Linkable {
        icon: kind_icon(Kind::Article),
        title: title.to_owned(),
        about: format!("Article · #{number}"),
        url: format!("cydonia://{project}#{number}"),
        touched: 0,
        project: project.to_owned(),
        active,
        number,
        key: key.map(str::to_owned),
        labels: Vec::new(),
        agent: None,
        archived: false,
        kind: match key {
            Some(_) => Kind::Board,
            None => Kind::Article,
        },
        row: Row::Entry {
            project: project.into(),
            showing: Showing::Article(number.to_string()),
        },
    }
}

fn held() -> Vec<Linkable> {
    vec![
        entry("cydonia", true, 120, "Notes on 12 things", None),
        entry("cydonia", true, 112, "Roadmap", None),
        entry("cydonia", true, 12, "Design", None),
        entry("cydonia", true, 9, "Development", Some("DEV")),
        entry("bezel", false, 112, "Input bugs", None),
        entry("bezel", false, 12, "Bezel board", Some("DEV")),
    ]
}

#[test]
fn a_hash_number_lists_the_active_project_by_number_exact_first() {
    assert_eq!(rank("#12", &held()), vec![2, 0]);
}

#[test]
fn a_bare_number_leads_with_references_then_titles() {
    // #12 exact, then #120 by prefix and by its title. #112 neither starts
    // with 12 nor holds it in a title.
    assert_eq!(rank("12", &held()), vec![2, 0]);
}

#[test]
fn a_project_prefix_scopes_to_that_project() {
    assert_eq!(rank("bezel#", &held()), vec![4, 5]);
    assert_eq!(rank("bezel#112", &held()), vec![4]);
    assert_eq!(rank("Bezel#1", &held()), vec![4, 5]);
}

#[test]
fn an_unknown_project_or_a_non_number_lists_nothing() {
    assert!(rank("nowhere#1", &held()).is_empty());
    assert!(rank("#abc", &held()).is_empty());
}

#[test]
fn a_board_key_matches_before_titles() {
    assert_eq!(rank("dev", &held()), vec![3]);
    assert_eq!(rank("bezel#DE", &held()), vec![5]);
}

#[test]
fn an_empty_query_lists_only_the_active_project() {
    assert_eq!(rank("", &held()), vec![0, 1, 2, 3]);
}

#[test]
fn references_list_no_titles() {
    assert_eq!(references("12", &held()), vec![2, 0]);
    assert_eq!(references("bezel#dev", &held()), vec![5]);
    assert!(references("Design", &held()).is_empty());
    assert!(references("", &held()).is_empty());
}

#[test]
fn a_title_starting_with_the_query_outranks_a_key_prefix() {
    let mut held = held();
    held.push(entry("cydonia", true, 30, "Roadmap board", Some("PXL")));
    held.push(entry("cydonia", true, 31, "Px notes", None));
    assert_eq!(rank("px", &held), vec![7, 6]);
}

#[test]
fn titles_match_every_project_the_active_one_first() {
    let mut held = held();
    held.push(entry("cydonia", true, 30, "Board games", None));
    assert_eq!(rank("board", &held), vec![6, 5]);
}

#[test]
fn archived_entries_follow_every_other_match_but_an_exact_reference() {
    let mut held = held();
    held[2].archived = true;
    // Design (#12) is archived: still first by its exact number, but last
    // when only its title matches.
    assert_eq!(rank("12", &held), vec![2, 0]);
    held.push(entry("bezel", false, 40, "Design review", None));
    assert_eq!(rank("design", &held), vec![6, 2]);
    assert_eq!(rank("", &held), vec![0, 1, 3, 2]);
}

#[test]
fn a_kind_prefix_lists_that_kind_alone() {
    assert_eq!(rank("b:", &held()), vec![3]);
    assert_eq!(rank("B:dev", &held()), vec![3]);
    assert_eq!(rank("a:12", &held()), vec![2, 0]);
    assert!(rank("s:", &held()).is_empty());
    assert_eq!(kind_prefix("x:12"), (None, "x:12"));
}

#[test]
fn a_label_prefix_lists_the_entries_carrying_it_in_every_project() {
    let mut held = held();
    held[1].labels = vec!["q3".into()];
    held[4].labels = vec!["q3".into(), "research".into()];
    assert_eq!(rank("l:Q3", &held), vec![1, 4]);
    assert_eq!(rank("l:q3 input", &held), vec![4]);
    assert_eq!(rank("a:l:research", &held), vec![4]);
    assert_eq!(label_prefix("l: x"), (None, "l: x"));
}
