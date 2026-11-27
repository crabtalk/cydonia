use serde::{Deserialize, Serialize};
use std::fmt;

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

    /// The preset's name; `None` for any other size.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::DESKTOP => Some("Desktop"),
            Self::TABLET => Some("Tablet"),
            Self::PHONE => Some("Phone"),
            _ => None,
        }
    }
}

impl fmt::Display for Viewport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} × {}", self.width, self.height)
    }
}
