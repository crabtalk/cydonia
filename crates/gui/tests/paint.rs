//! A colour picked in settings, as `settings.toml` holds it.

use cydonia_gui::model::settings::{Appearance, Highlight, Paint};

#[test]
fn a_name_and_a_hex_both_read_back_as_written() {
    for paint in [
        Paint::Named(Highlight::Indigo),
        Paint::Custom {
            rgb: 0x12ab3c,
            alpha: None,
        },
        Paint::Custom {
            rgb: 0x12ab3c,
            alpha: Some(0x70),
        },
    ] {
        assert_eq!(Paint::parse(&paint.key()), Some(paint));
    }
    let blue = |alpha| Paint::Custom {
        rgb: 0x0000ff,
        alpha,
    };
    assert_eq!(blue(None).key(), "#0000ff");
    assert_eq!(blue(Some(0x80)).key(), "#0000ff80");
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
    assert_eq!(
        look.caret,
        Some(Paint::Custom {
            rgb: 0xff8800,
            alpha: None
        })
    );
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
fn a_custom_wash_paints_its_own_alpha() {
    use bezel::theme::{Appearance as Mode, Theme};
    for mode in [Mode::Light, Mode::Dark] {
        let theme = Theme::for_appearance(mode);
        let picked = bezel::gpui::Hsla::from(bezel::gpui::rgba(0x3366ffcc));
        let paint = Paint::from_hsla(picked, true);
        assert_eq!(paint.key(), "#3366ffcc");
        assert!((paint.wash(&theme).a - picked.a).abs() < 1e-3);
        let opaque = Paint::from_hsla(picked, false);
        assert_eq!(opaque.key(), "#3366ff");
        assert_eq!(opaque.solid(&theme).a, 1.);
    }
}
