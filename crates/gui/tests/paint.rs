//! A colour picked in settings, as `settings.toml` holds it.

use cydonia_gui::model::settings::{Appearance, Highlight, Paint};

#[test]
fn a_name_and_a_hex_both_read_back_as_written() {
    for paint in [
        Paint::Named(Highlight::Indigo),
        Paint::Custom { rgb: 0x12ab3c },
    ] {
        assert_eq!(Paint::parse(&paint.key()), Some(paint));
    }
    assert_eq!(Paint::Custom { rgb: 0x0000ff }.key(), "#0000ff");
}

#[test]
fn a_hex_with_alpha_reads_as_its_colour() {
    let paint = Paint::parse("#0000ff80").unwrap();
    assert_eq!(paint, Paint::Custom { rgb: 0x0000ff });
    assert_eq!(paint.key(), "#0000ff");
}

#[test]
fn a_highlight_written_as_a_name_still_reads() {
    let look: Appearance = toml::from_str("highlight = \"green\"").unwrap();
    assert_eq!(look.highlight, Paint::Named(Highlight::Green));
}

#[test]
fn a_file_written_before_custom_colours_still_reads() {
    let look: Appearance = toml::from_str("selection = \"pink\"\ncaret = \"#ff8800\"").unwrap();
    assert_eq!(look.selection, Some(Paint::Named(Highlight::Pink)));
    assert_eq!(look.caret, Some(Paint::Custom { rgb: 0xff8800 }));
}

#[test]
fn a_value_that_is_not_a_colour_reads_as_the_default() {
    let look: Appearance =
        toml::from_str("highlight = \"#be4b637\"\nselection = \"teal-ish\"\ncaret = 3").unwrap();
    assert_eq!(look.highlight, Paint::Named(Highlight::Yellow));
    assert_eq!(look.selection, None);
    assert_eq!(look.caret, None);
}

#[test]
fn neither_a_short_hex_nor_an_unknown_name_is_a_colour() {
    assert_eq!(Paint::parse("#fff"), None);
    assert_eq!(Paint::parse("#ff88008"), None);
    assert_eq!(Paint::parse("magenta"), None);
}

#[test]
fn every_preset_is_its_swatch() {
    use bezel::theme::{Appearance as Mode, Theme};
    let swatches = bezel::ui::color::default_swatches();
    assert_eq!(swatches.len(), Highlight::ALL.len());
    for mode in [Mode::Light, Mode::Dark] {
        let theme = Theme::for_appearance(mode);
        for (named, swatch) in Highlight::ALL.into_iter().zip(swatches.iter()) {
            assert_eq!(named.key(), swatch.name.to_lowercase());
            assert_eq!(named.solid(&theme), swatch.resolve(&theme));
        }
    }
}

#[test]
fn a_picked_colour_paints_opaque_whatever_its_alpha() {
    use bezel::theme::{Appearance as Mode, Theme};
    for mode in [Mode::Light, Mode::Dark] {
        let theme = Theme::for_appearance(mode);
        let picked = bezel::gpui::Hsla::from(bezel::gpui::rgba(0x3366ffcc));
        let paint = Paint::from_hsla(picked);
        assert_eq!(paint.key(), "#3366ff");
        assert_eq!(paint.solid(&theme).a, 1.);
    }
}
