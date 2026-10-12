//! Where the margin stacks its notes: level with their lines where there is
//! room, pushed under the note above where there is not.

use super::*;

fn tops(wants: &[f32], heights: &[f32], pinned: Option<usize>) -> Vec<f32> {
    let wants: Vec<_> = wants.iter().map(|top| px(*top)).collect();
    let heights: Vec<_> = heights.iter().map(|height| px(*height)).collect();
    stack(&wants, &heights, pinned)
        .into_iter()
        .map(f32::from)
        .collect()
}

#[test]
fn notes_with_room_sit_at_their_lines() {
    assert_eq!(tops(&[0., 100.], &[40., 40.], None), vec![0., 100.]);
}

#[test]
fn a_note_that_would_overlap_goes_under_the_one_above() {
    assert_eq!(
        tops(&[0., 10., 20.], &[40., 40., 40.], None),
        vec![0., 48., 96.]
    );
}

#[test]
fn the_pinned_note_sits_at_its_line_and_the_others_make_room() {
    assert_eq!(
        tops(&[0., 10., 20.], &[40., 40., 40.], Some(1)),
        vec![-38., 10., 58.]
    );
}
