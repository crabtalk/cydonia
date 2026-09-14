//! What the Reduce-transparency switch asks of the theme.

use bezel::theme::Vibrancy;
use cydonia::model::workspace::vibrancy;

/// Nothing said is not "no": it is the answer bezel resolves per appearance,
/// which is opaque in light and frosted in dark. A fresh install has said
/// nothing, so this is what almost everyone runs.
#[test]
fn an_unpressed_switch_leaves_the_answer_to_the_appearance() {
    assert_eq!(vibrancy(None), Vibrancy::Auto);
}

/// From the first press it is theirs, in both appearances — including the
/// light one it would otherwise have held opaque.
#[test]
fn a_pressed_switch_is_the_answer_in_both_appearances() {
    assert_eq!(vibrancy(Some(true)), Vibrancy::Off);
    assert_eq!(vibrancy(Some(false)), Vibrancy::On);
}
