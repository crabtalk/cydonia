//! What the Reduce-transparency switch asks of the theme.

use bezel::theme::{LENSED, Vibrancy};
use cydonia_gui::model::workspace::{glass, vibrancy};

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

/// Glass is the switch's other answer, and it is not the window's: an opaque
/// light window still paints translucent cards.
#[test]
fn an_unpressed_switch_leaves_the_components_glassed() {
    assert_eq!(glass(None), LENSED);
    assert_eq!(glass(Some(false)), LENSED);
}

/// Pressed, the components go flat in both appearances — which is the only
/// thing the switch has to say in light.
#[test]
fn a_pressed_switch_takes_the_glass_off_the_components() {
    assert!(!glass(Some(true)));
}
