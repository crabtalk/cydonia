//! Which appearances the window composites opaque.

use bezel::theme::Appearance;
use cydonia::model::workspace::opaque;

/// Light has no vibrancy to keep, so the switch has nothing to say there.
#[test]
fn light_is_opaque_whatever_the_switch_says() {
    assert!(opaque(false, Appearance::Light));
    assert!(opaque(true, Appearance::Light));
}

/// Dark is the one appearance the frost is painted for, so it is the one the
/// switch decides.
#[test]
fn dark_is_the_switch() {
    assert!(!opaque(false, Appearance::Dark));
    assert!(opaque(true, Appearance::Dark));
}
