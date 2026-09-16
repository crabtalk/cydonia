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

/// Pressed, the window is opaque in both appearances.
#[test]
fn a_pressed_switch_holds_the_window_opaque_in_both_appearances() {
    assert_eq!(vibrancy(Some(true)), Vibrancy::Off);
}

/// Released, it asks for frost where there is a palette for one, which is dark
/// alone. Never [`Vibrancy::On`]: light frosted is the combination that paints
/// text over the desktop.
#[test]
fn a_released_switch_never_frosts_light() {
    assert_eq!(vibrancy(Some(false)), Vibrancy::Auto);
}
