//! Which project a reference's name reaches.

mod common;

use common::{Rail, Scratch};
use cydonia_mcp::tool::{Args, Trouble};
use cydonia_mcp::tools::project_of;
use serde_json::json;

fn reach(text: &str, at: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let reference = artifact::reference::parse(text).unwrap();
    let arguments = json!({});
    project_of(&Args::new(&arguments, Some(at)), &reference).map_err(|trouble| match trouble {
        Trouble::Refused(why) | Trouble::Invalid(why) => why,
    })
}

#[test]
fn no_name_is_the_project_the_call_is_about() {
    let here = Scratch::new("ref-here");
    assert_eq!(reach("#43", here.path()).unwrap(), here.path());
}

#[test]
fn a_name_reaches_the_open_project_of_that_name() {
    let here = Scratch::new("ref-from");
    let there = Scratch::new("ref-to");
    let foo = there.path().join("foo");
    std::fs::create_dir(&foo).unwrap();
    Rail::holding(&[here.path(), &foo]);

    assert_eq!(reach("foo#43:5-7", here.path()).unwrap(), foo);
    assert_eq!(reach("foo#DEV-12", here.path()).unwrap(), foo);
}

#[test]
fn a_name_nothing_open_answers_to_is_refused() {
    let here = Scratch::new("ref-none");
    Rail::holding(&[here.path()]);

    let why = reach("foo#43", here.path()).unwrap_err();
    assert!(why.contains("no open project is named foo"), "{why}");
    assert!(why.contains(&here.path().display().to_string()), "{why}");
}

#[test]
fn a_name_two_open_projects_share_is_refused_with_both() {
    let one = Scratch::new("ref-one");
    let two = Scratch::new("ref-two");
    let (a, b) = (one.path().join("foo"), two.path().join("foo"));
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    Rail::holding(&[&a, &b]);

    let why = reach("foo#43", one.path()).unwrap_err();
    assert!(why.contains("more than one"), "{why}");
    assert!(why.contains(&a.display().to_string()), "{why}");
    assert!(why.contains(&b.display().to_string()), "{why}");
}
