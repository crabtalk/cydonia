use super::*;

#[test]
fn old_settings_keep_article_inheritance_and_the_mono_default() {
    let look: Appearance = toml::from_str("text_size = 16.0").unwrap();
    assert_eq!(look.article_font_size, None);
    assert_eq!(look.mono_font_size, 13.);
    assert_eq!(look.text_size, 16.);
}

#[test]
fn a_file_written_before_the_merge_keeps_its_terminal_size() {
    let look: Appearance =
        toml::from_str("terminal_font_size = 15.0\nfile_font_size = 20.0").unwrap();
    assert_eq!(look.mono_font_size, 15.);

    // And the keys it replaced go out of the file rather than sitting there
    // reading as switches.
    let mut doc: toml_edit::DocumentMut =
        "[appearance]\nterminal_font_size = 15.0\nfile_font_size = 20.0\n"
            .parse()
            .unwrap();
    write_appearance(&mut doc, &look).unwrap();
    let body = doc.to_string();
    assert!(body.contains("mono_font_size = 15.0"));
    assert!(!body.contains("terminal_font_size"));
    assert!(!body.contains("file_font_size"));
}

#[test]
fn sizes_round_trip_without_losing_comments_or_unrelated_settings() {
    let mut doc: toml_edit::DocumentMut = "# My settings\n[appearance]\n# Preferred body size\ntext_size = 13.0\ncustom_setting = true\n".parse().unwrap();
    let look = Appearance {
        article_font_size: Some(18.),
        mono_font_size: 15.,
        ..Appearance::default()
    };
    write_appearance(&mut doc, &look).unwrap();
    let body = doc.to_string();
    assert!(body.contains("# My settings"));
    assert!(body.contains("# Preferred body size"));
    assert!(body.contains("custom_setting = true"));
    let read: Settings = toml::from_str(&body).unwrap();
    assert_eq!(read.appearance, look);
}

#[test]
fn invalid_font_sizes_are_normalized_before_layout() {
    let mut look: Appearance =
        toml::from_str("text_size = nan\narticle_font_size = -2.0\nmono_font_size = inf").unwrap();
    look.normalize();
    assert!(look.text_size.is_finite());
    assert_eq!(look.article_font_size, Some(CONTENT_TEXT_SIZE.0));
    assert_eq!(look.mono_font_size, 13.);
    look.mono_font_size = 1000.;
    look.normalize();
    assert_eq!(look.mono_font_size, CONTENT_TEXT_SIZE.1);
}

#[test]
fn families_round_trip_and_an_empty_one_is_no_family_at_all() {
    let mut doc: toml_edit::DocumentMut =
        "[appearance]\nui_font = \"Helvetica\"\n".parse().unwrap();
    let look = Appearance {
        ui_font: Some("Inter".into()),
        article_font: Some("Charter".into()),
        mono_font: Some("Fira Code".into()),
        ..Appearance::default()
    };
    write_appearance(&mut doc, &look).unwrap();
    let read: Settings = toml::from_str(&doc.to_string()).unwrap();
    assert_eq!(read.appearance, look);

    // Unset takes the key back out rather than writing an empty string.
    write_appearance(&mut doc, &Appearance::default()).unwrap();
    let body = doc.to_string();
    assert!(!body.contains("ui_font ="));
    assert!(!body.contains("article_font ="));
    assert!(!body.contains("mono_font ="));

    let mut blank: Appearance =
        toml::from_str("ui_font = \"\"\narticle_font = \" \"\nmono_font = \"  \"").unwrap();
    blank.normalize();
    assert_eq!(blank.ui_font, None);
    assert_eq!(blank.article_font, None);
    assert_eq!(blank.mono_font, None);
}
