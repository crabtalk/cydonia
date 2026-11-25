use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::DESKTOP
    }
}

impl Viewport {
    pub const DESKTOP: Self = Self {
        width: 1440,
        height: 900,
    };
    pub const TABLET: Self = Self {
        width: 768,
        height: 1024,
    };
    pub const PHONE: Self = Self {
        width: 390,
        height: 844,
    };
    pub const PRESETS: [Self; 3] = [Self::DESKTOP, Self::TABLET, Self::PHONE];
}
