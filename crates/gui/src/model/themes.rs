//! The theme families cydonia ships, compiled in from `assets/themes/`.

use bezel::theme::ThemeFamily;
use std::sync::LazyLock;

const FILES: [&str; 9] = [
    include_str!("../../assets/themes/ayu.toml"),
    include_str!("../../assets/themes/catppuccin.toml"),
    include_str!("../../assets/themes/dracula.toml"),
    include_str!("../../assets/themes/everforest.toml"),
    include_str!("../../assets/themes/github.toml"),
    include_str!("../../assets/themes/gruvbox.toml"),
    include_str!("../../assets/themes/one.toml"),
    include_str!("../../assets/themes/solarized.toml"),
    include_str!("../../assets/themes/tokyo-night.toml"),
];

static FAMILIES: LazyLock<Vec<ThemeFamily>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|file| toml::from_str(file).expect("bundled theme"))
        .collect()
});

/// Every bundled family, in the order the picker lists them.
pub fn all() -> &'static [ThemeFamily] {
    &FAMILIES
}

/// The family `name` names, ignoring case. `None` for a name no family has,
/// which paints the default palette.
pub fn named(name: &str) -> Option<&'static ThemeFamily> {
    all()
        .iter()
        .find(|family| family.name.eq_ignore_ascii_case(name))
}
