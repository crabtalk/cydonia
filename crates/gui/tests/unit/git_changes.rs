use super::*;

#[test]
fn wrapped_unicode_token_keeps_highlights_on_both_continuations() {
    let source = "let 名称 = \"你好世界\";";
    let token = source.find('"').unwrap()..source.rfind('"').unwrap() + 1;
    let split = source.find('世').unwrap();
    let segments = [0..split, split..source.len()];
    let highlighted: String = segments
        .iter()
        .map(|segment| {
            let local = clipped_range(&token, segment).unwrap();
            source[segment.clone()][local].to_owned()
        })
        .collect();
    assert_eq!(highlighted, "\"你好世界\"");
    assert_eq!(clipped_range(&(0..3), &segments[1]), None);
}

fn luminance(color: Hsla) -> f32 {
    let color: gpui::Rgba = color.into();
    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

#[test]
fn diff_backgrounds_keep_code_contrast_in_both_appearances() {
    for theme in [Theme::dark(), Theme::light()] {
        for tone in [theme.diff_add, theme.diff_del] {
            let background = diff_wash(&theme, tone);
            assert_eq!(
                background.a, 1.0,
                "Code contrast must not depend on the desktop behind it"
            );
            let foreground = luminance(theme.text);
            let background = luminance(background);
            let contrast =
                (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
            assert!(contrast >= 7.0, "Code contrast is {contrast}:1");
        }
    }
}
