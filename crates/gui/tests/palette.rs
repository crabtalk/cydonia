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
            let Some(variant) = family.variant(appearance) else {
                continue;
            };
            let mut theme = Theme::for_appearance(appearance);
            let unknown = variant.apply(&mut theme);
            assert!(
                unknown.is_empty(),
                "{} {appearance:?}: {unknown:?}",
                family.name
            );
        }
    }
}

#[test]
fn each_appearance_takes_its_own_theme_and_a_missing_variant_is_the_default() {
    let look = settings::Appearance {
        theme: settings::Themes {
            light: Some("gruvbox".into()),
            dark: Some("Lobster".into()),
        },
        ..settings::Appearance::default()
    };
    let inputs = Inputs::of(&look, Families::default());
    let gruvbox = themes::named("Gruvbox").unwrap();
    let light = build(&inputs, Appearance::Light);
    assert_eq!(Some(light.bg), gruvbox.light.as_ref().unwrap().get("bg"));
    let dark = build(&inputs, Appearance::Dark);
    assert_eq!(dark.family.as_deref(), Some("Lobster"));

    // Lobster has no light variant.
    let look = settings::Appearance {
        theme: settings::Themes {
            light: Some("Lobster".into()),
            dark: Some("no such theme".into()),
        },
        ..settings::Appearance::default()
    };
    let inputs = Inputs::of(&look, Families::default());
    assert_eq!(build(&inputs, Appearance::Light).bg, Theme::light().bg);
    assert_eq!(build(&inputs, Appearance::Dark).bg, Theme::dark().bg);
}

#[test]
fn a_bare_theme_name_paints_both_appearances() {
    let both: settings::Appearance = toml::from_str(r#"theme = "Nord""#).unwrap();
    assert_eq!(both.theme.light.as_deref(), Some("Nord"));
    assert_eq!(both.theme.dark.as_deref(), Some("Nord"));
    let split: settings::Appearance = toml::from_str(r#"theme = { dark = "Lobster" }"#).unwrap();
    assert_eq!(split.theme.light, None);
    assert_eq!(split.theme.dark.as_deref(), Some("Lobster"));
}
