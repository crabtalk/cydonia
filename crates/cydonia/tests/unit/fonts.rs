use super::*;

#[test]
fn chosen_families_reach_the_palette_and_unset_leaves_it_alone() {
    let shipped = Theme::for_appearance(Appearance::Dark);
    let plain = palette(Appearance::Dark);
    assert_eq!(plain.font_sans, shipped.font_sans);
    assert_eq!(plain.font_mono, shipped.font_mono);

    init(Families {
        sans: Some("Inter".into()),
        body: None,
        mono: None,
    });
    let picked = palette(Appearance::Dark);
    assert_eq!(picked.font_sans, SharedString::from("Inter"));
    // Prose follows the interface family until it is given one of its own.
    assert_eq!(picked.font_body, SharedString::from("Inter"));
    // The slot nobody answered keeps the palette's own face, which is what a
    // family the reader never picked has to mean.
    assert_eq!(picked.font_mono, shipped.font_mono);

    init(Families {
        sans: Some("Inter".into()),
        body: Some("Charter".into()),
        mono: None,
    });
    let split = palette(Appearance::Dark);
    assert_eq!(split.font_sans, SharedString::from("Inter"));
    assert_eq!(split.font_body, SharedString::from("Charter"));
}
