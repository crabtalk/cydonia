use super::*;

#[test]
fn old_settings_keep_article_inheritance_and_terminal_default() {
    let look: Appearance = toml::from_str("text_size = 16.0").unwrap();
    assert_eq!(look.file_font_size, TextStyle::Body.size());
    assert_eq!(look.article_font_size, None);
    assert_eq!(look.terminal_font_size, 13.);
    assert_eq!(look.text_size, 16.);
}

#[test]
fn sizes_round_trip_without_losing_comments_or_unrelated_settings() {
    let mut doc: toml_edit::DocumentMut = "# My settings\n[appearance]\n# Preferred terminal size\nterminal_font_size = 13.0\ncustom_setting = true\n".parse().unwrap();
    let look = Appearance {
        file_font_size: 20.,
        article_font_size: Some(18.),
        terminal_font_size: 15.,
        ..Appearance::default()
    };
    write_appearance(&mut doc, &look).unwrap();
    let body = doc.to_string();
    assert!(body.contains("# My settings"));
    assert!(body.contains("# Preferred terminal size"));
    assert!(body.contains("custom_setting = true"));
    let read: Settings = toml::from_str(&body).unwrap();
    assert_eq!(read.appearance, look);
}

#[test]
fn invalid_font_sizes_are_normalized_before_layout() {
    let mut look: Appearance =
        toml::from_str("text_size = nan\narticle_font_size = -2.0\nterminal_font_size = inf\nfile_font_size = -10.0")
            .unwrap();
    look.normalize();
    assert!(look.text_size.is_finite());
    assert_eq!(look.file_font_size, CONTENT_TEXT_SIZE.0);
    assert_eq!(look.article_font_size, Some(CONTENT_TEXT_SIZE.0));
    assert_eq!(look.terminal_font_size, 13.);
    look.terminal_font_size = 1000.;
    look.normalize();
    assert_eq!(look.terminal_font_size, CONTENT_TEXT_SIZE.1);
}
