//! A colour picked in settings, as `settings.toml` holds it.

use cydonia_gui::model::settings::{Appearance, Highlight, Paint};

#[test]
fn a_name_and_a_hex_both_read_back_as_written() {
    for paint in [Paint::Named(Highlight::Blue), Paint::Custom(0x12ab3c)] {
        assert_eq!(Paint::parse(&paint.key()), Some(paint));
    }
    assert_eq!(Paint::Custom(0x0000ff).key(), "#0000ff");
}

#[test]
fn a_file_written_before_custom_colours_still_reads() {
    let look: Appearance = toml::from_str("selection = \"pink\"\ncaret = \"#ff8800\"").unwrap();
    assert_eq!(look.selection, Some(Paint::Named(Highlight::Pink)));
    assert_eq!(look.caret, Some(Paint::Custom(0xff8800)));
}

#[test]
fn neither_a_short_hex_nor_an_unknown_name_is_a_colour() {
    assert_eq!(Paint::parse("#fff"), None);
    assert_eq!(Paint::parse("teal"), None);
}

#[test]
fn every_preset_reads_back_as_its_own_hex() {
    use bezel::theme::{Appearance as Mode, Theme};
    let theme = Theme::for_appearance(Mode::Dark);
    for swatch in bezel::ui::color::default_swatches().iter() {
        let paint = Paint::from_hsla(swatch.resolve(&theme));
        assert_eq!(
            Paint::from_hsla(paint.solid(&theme)),
            paint,
            "{}",
            swatch.name
        );
    }
    let red = &bezel::ui::color::default_swatches()[0];
    assert_eq!(
        Paint::from_hsla(red.resolve(&theme)),
        Paint::Custom(0xff453a)
    );
}
