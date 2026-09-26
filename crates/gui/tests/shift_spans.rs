//! Syntax spans carried over an edit until the next parse replaces them.

use cydonia_gui::view::component::file::shift_spans;

#[test]
fn spans_after_an_insert_move_by_its_length() {
    // `fn main` with `fn` at 0..2 and `main` at 3..7; type `pub ` in front.
    let spans = [(0..2, 'k'), (3..7, 'f')];
    let moved = shift_spans(&spans, "fn main", "pub fn main");
    assert_eq!(moved, vec![(4..6, 'k'), (7..11, 'f')]);
}

#[test]
fn spans_before_an_edit_stay_put() {
    let spans = [(0..2, 'k'), (3..7, 'f')];
    let moved = shift_spans(&spans, "fn main", "fn main()");
    assert_eq!(moved, vec![(0..2, 'k'), (3..7, 'f')]);
}

#[test]
fn a_span_typed_inside_grows_with_it() {
    let spans = [(3..7, 'f')];
    let moved = shift_spans(&spans, "fn main", "fn maXYin");
    assert_eq!(moved, vec![(3..9, 'f')]);
    let moved = shift_spans(&spans, "fn main", "fn mn");
    assert_eq!(moved, vec![(3..5, 'f')]);
}

#[test]
fn a_span_an_edit_cuts_into_keeps_what_lies_outside() {
    // Replace `n ma` (1..5) with `X`: `fn` keeps `f`, `main` keeps `in`.
    let spans = [(0..2, 'k'), (3..7, 'f')];
    let moved = shift_spans(&spans, "fn main", "fXin");
    assert_eq!(moved, vec![(0..1, 'k'), (2..4, 'f')]);
}

#[test]
fn a_deleted_span_is_dropped() {
    let spans = [(0..2, 'k'), (3..7, 'f'), (7..9, 'p')];
    let moved = shift_spans(&spans, "fn main()", "fn ()");
    assert_eq!(moved, vec![(0..2, 'k'), (3..5, 'p')]);
}

#[test]
fn edits_between_multibyte_characters_stay_on_boundaries() {
    // `"é" "é"`: each `é` is two bytes, so the strings are 0..4 and 5..9.
    let spans = [(0..4, 's'), (5..9, 's')];
    // Insert the two-byte `ü` after the space.
    let moved = shift_spans(&spans, "\"é\" \"é\"", "\"é\" ü\"é\"");
    assert_eq!(moved, vec![(0..4, 's'), (7..11, 's')]);
    // Replace one `é` with another two-byte character sharing its lead byte.
    let moved = shift_spans(&spans, "\"é\" \"é\"", "\"é\" \"è\"");
    assert_eq!(moved, vec![(0..4, 's'), (5..9, 's')]);
}
