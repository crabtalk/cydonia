//! How a tool call's label is cut into the head, the rest, and what is left to
//! open the row onto.

use cydonia_gui::view::component::transcript::{HEAD_MAX, title};

/// A shell script arrives as its own title. The row gets one line of it;
/// the whole thing is what opening the row is for.
#[test]
fn a_script_is_one_line_and_a_way_back_to_the_rest() {
    let script = "python3 -c \"\nimport json\nprint(json.dumps({}))\n\"";
    let (name, rest, full) = title(script);
    assert_eq!(name, "python3");
    assert_eq!(
        rest.expect("the command after the head"),
        "-c \" import json print(json.dumps({})) \""
    );
    assert_eq!(full.expect("the script, as written"), script);
}

/// A title that already fits keeps its head and its rest, and has nothing
/// left over to open onto.
#[test]
fn a_short_title_opens_onto_nothing() {
    let (name, rest, full) = title("Read src/view/root.rs");
    assert_eq!(name, "Read");
    assert_eq!(rest.expect("the path"), "src/view/root.rs");
    assert!(full.is_none());
}

/// One long word is still one line: the head is capped so the rest has
/// somewhere to truncate.
#[test]
fn a_head_longer_than_the_row_is_cut() {
    let (name, rest, _) = title(&"x".repeat(40));
    assert_eq!(name.len(), HEAD_MAX);
    assert_eq!(rest.expect("what the head could not take").len(), 16);
}
