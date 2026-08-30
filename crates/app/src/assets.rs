//! What `svg()` and `img()` can reach by name: bezel's shipped icons, plus the
//! agent icons [`crate::agents`] has cached.
//!
//! A cached icon's asset path is its own path on disk, so the fallback is a
//! read rather than a table — there is nothing to keep in sync.

use anyhow::Result;
use bezel::{
    gpui::{AssetSource, SharedString},
    ui::icons,
};
use std::borrow::Cow;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = icons::Assets.load(path)? {
            return Ok(Some(bytes));
        }
        Ok(std::fs::read(path).ok().map(Cow::Owned))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        icons::Assets.list(path)
    }
}
