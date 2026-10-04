//! What `@` lists for a query: references by number and project, board keys,
//! then titles.

use super::*;

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
    // #12 exact, #120 by prefix, then "Notes on 12 things" is already listed
    // and #112 matches only through its description.
    assert_eq!(rank("12", &held()), vec![2, 0, 1]);
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
