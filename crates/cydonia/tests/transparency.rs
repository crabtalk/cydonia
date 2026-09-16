//! What the Reduce-transparency switch asks of the theme.

use bezel::theme::Vibrancy;
use cydonia::model::workspace::vibrancy;

/// Unset resolves per appearance: opaque in light, frosted in dark.
#[test]
fn an_unpressed_switch_leaves_the_answer_to_the_appearance() {
    assert_eq!(vibrancy(None), Vibrancy::Auto);
}

/// Pressed, the window is opaque in both appearances.
#[test]
fn a_pressed_switch_holds_the_window_opaque_in_both_appearances() {
    assert_eq!(vibrancy(Some(true)), Vibrancy::Off);
}

/// Released, frost in dark and opaque in light. Never `Vibrancy::On`.
#[test]
fn a_released_switch_never_frosts_light() {
    assert_eq!(vibrancy(Some(false)), Vibrancy::Auto);
}
