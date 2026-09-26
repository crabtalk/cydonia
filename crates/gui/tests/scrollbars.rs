use cydonia_gui::model::settings::{Appearance, Scrollbars, Settings};

#[test]
fn existing_settings_default_to_hidden_sidebar_scrollbars() {
    let settings: Settings = toml::from_str("[appearance]\nindent_project_rows = false\n").unwrap();
    assert_eq!(settings.appearance.scrollbars, Scrollbars::Scrolling);
    assert_eq!(settings.appearance.sidebar_scrollbars, Scrollbars::Never);
    assert!(!settings.appearance.indent_project_rows);
}

#[test]
fn sidebar_visibility_is_independent_and_survives_serialization() {
    for content in Scrollbars::ALL {
        for sidebar in Scrollbars::ALL {
            let appearance = Appearance {
                scrollbars: content,
                sidebar_scrollbars: sidebar,
                ..Appearance::default()
            };
            let body = toml::to_string(&appearance).unwrap();
            let loaded: Appearance = toml::from_str(&body).unwrap();
            assert_eq!(loaded, appearance);
            assert!(body.contains(&format!("scrollbars = \"{}\"", content.key())));
            assert!(body.contains(&format!("sidebar_scrollbars = \"{}\"", sidebar.key())));
        }
    }
}
