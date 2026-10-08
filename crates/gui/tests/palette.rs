use bezel::{
    gpui::SharedString,
    theme::{Appearance, Theme},
};
use cydonia_gui::model::{
    fonts::Families,
    palette::{Inputs, build},
    settings, themes,
};

#[test]
fn chosen_families_reach_the_palette_and_unset_leaves_it_alone() {
    let shipped = Theme::for_appearance(Appearance::Dark);
    let plain = build(&Inputs::default(), Appearance::Dark);
    assert_eq!(plain.font_sans, shipped.font_sans);
    assert_eq!(plain.font_mono, shipped.font_mono);

    let picked = build(
        &Inputs {
            families: Families {
                sans: Some("Inter".into()),
                body: None,
                mono: None,
            },
            ..Inputs::default()
        },
        Appearance::Dark,
    );
    assert_eq!(picked.font_sans, SharedString::from("Inter"));
    // Prose follows the interface family until it is given one of its own.
    assert_eq!(picked.font_body, SharedString::from("Inter"));
    // The slot nobody answered keeps the palette's own face.
    assert_eq!(picked.font_mono, shipped.font_mono);

    let split = build(
        &Inputs {
            families: Families {
                sans: Some("Inter".into()),
                body: Some("Charter".into()),
                mono: None,
            },
            ..Inputs::default()
        },
        Appearance::Dark,
    );
    assert_eq!(split.font_sans, SharedString::from("Inter"));
    assert_eq!(split.font_body, SharedString::from("Charter"));
}

#[test]
fn every_bundled_theme_names_only_tokens() {
    assert!(!themes::all().is_empty());
    for family in themes::all() {
        for appearance in [Appearance::Dark, Appearance::Light] {
            let mut theme = Theme::for_appearance(appearance);
            let unknown = family.variant(appearance).apply(&mut theme);
            assert!(
                unknown.is_empty(),
                "{} {appearance:?}: {unknown:?}",
                family.name
            );
        }
    }
}

#[test]
fn a_named_theme_reaches_the_palette_and_an_unknown_one_is_the_default() {
    let look = settings::Appearance {
        theme: Some("gruvbox".into()),
        ..settings::Appearance::default()
    };
    let gruvbox = themes::named("Gruvbox").unwrap();
    let theme = build(&Inputs::of(&look, Families::default()), Appearance::Light);
    assert_eq!(Some(theme.bg), gruvbox.light.get("bg"));

    let look = settings::Appearance {
        theme: Some("no such theme".into()),
        ..settings::Appearance::default()
    };
    let theme = build(&Inputs::of(&look, Families::default()), Appearance::Dark);
    assert_eq!(theme.bg, Theme::dark().bg);
}
