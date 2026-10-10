//! The theme families cydonia ships, compiled in from `assets/themes/`.

use bezel::theme::ThemeFamily;
use std::sync::LazyLock;

const FILES: [&str; 27] = [
    include_str!("../../assets/themes/absolutely.toml"),
    include_str!("../../assets/themes/ayu.toml"),
    include_str!("../../assets/themes/catppuccin.toml"),
    include_str!("../../assets/themes/codex.toml"),
    include_str!("../../assets/themes/dracula.toml"),
    include_str!("../../assets/themes/everforest.toml"),
    include_str!("../../assets/themes/github.toml"),
    include_str!("../../assets/themes/gruvbox.toml"),
    include_str!("../../assets/themes/linear.toml"),
    include_str!("../../assets/themes/lobster.toml"),
    include_str!("../../assets/themes/material.toml"),
    include_str!("../../assets/themes/matrix.toml"),
    include_str!("../../assets/themes/monokai.toml"),
    include_str!("../../assets/themes/night-owl.toml"),
    include_str!("../../assets/themes/nord.toml"),
    include_str!("../../assets/themes/notion.toml"),
    include_str!("../../assets/themes/one.toml"),
    include_str!("../../assets/themes/oscurange.toml"),
    include_str!("../../assets/themes/proof.toml"),
    include_str!("../../assets/themes/raycast.toml"),
    include_str!("../../assets/themes/rose-pine.toml"),
    include_str!("../../assets/themes/sentry.toml"),
    include_str!("../../assets/themes/solarized.toml"),
    include_str!("../../assets/themes/temple.toml"),
    include_str!("../../assets/themes/tokyo-night.toml"),
    include_str!("../../assets/themes/vercel.toml"),
    include_str!("../../assets/themes/vscode-plus.toml"),
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
