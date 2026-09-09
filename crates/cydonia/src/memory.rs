//! What the process holds on purpose: the caches, and the ceiling they run
//! under.
//!
//! Global rather than owned by a view. A cover is painted from one place and
//! held for the life of the app, so the thing that holds it cannot live on the
//! pane that happened to draw it first.
//!
//! gpui's own asset cache is a `FxHashMap` that only ever grows — `fetch_asset`
//! inserts and nothing evicts, so every cover a session opens stays decoded
//! until the app quits. Handing [`Covers`] to `img()` takes those bytes off it:
//! a custom [`ImageCache`] loads through the asset *loader* and never touches
//! the global map.

use crate::model::cover;
use bezel::gpui::{
    App, AppContext as _, Asset as _, AssetLogger, Entity, Global, ImageAssetLoader, ImageCache,
    ImageCacheError, ImageCacheItem, RenderImage, Resource, Window, hash,
};
use futures::FutureExt as _;
use lru::LruCache;
use std::{num::NonZeroUsize, sync::Arc};

/// The ceiling covers run under until settings say otherwise, in bytes.
pub const DEFAULT_LIMIT: u64 = 200 * 1_000_000;

pub struct Cache {
    pub covers: Entity<Covers>,
}

impl Global for Cache {}

/// The decoded covers, most recently drawn last. Bounded in whole pictures
/// rather than bytes: a generated cover is [`cover::RASTER_BYTES`] exactly, so
/// the count the ceiling converts to is the real one for a project's own
/// covers, and near it for an imported picture of other proportions.
pub struct Covers(LruCache<u64, ImageCacheItem>);

/// How many covers fit under `limit`. At least one — a ceiling that admits
/// nothing would re-decode the open article on every frame.
fn capacity(limit: u64) -> NonZeroUsize {
    NonZeroUsize::new((limit / cover::RASTER_BYTES).max(1) as usize).expect("max(1) is non-zero")
}

impl Covers {
    /// What is decoded right now — the figure the performance section prints.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The ceiling in bytes, back from the capacity it was set as.
    pub fn limit(&self) -> u64 {
        self.0.cap().get() as u64 * cover::RASTER_BYTES
    }

    /// Move the ceiling. Whatever no longer fits goes now rather than on the
    /// next draw, so the figure beside the setting answers for the change.
    pub fn set_limit(&mut self, limit: u64, window: &mut Window, cx: &mut App) {
        // Before the resize, not after: `LruCache::resize` discards what no
        // longer fits without handing it back, and a frame dropped that way
        // leaves its sprite atlas texture behind.
        let cap = capacity(limit);
        while self.0.len() > cap.get() {
            let Some((_, mut item)) = self.0.pop_lru() else {
                break;
            };
            drop_frame(&mut item, window, cx);
        }
        self.0.resize(cap);
    }
}

/// Give a decoded cover back: the frame, and the sprite atlas texture each
/// window cut from it. Dropping the map entry alone leaves the second copy on
/// the GPU.
fn drop_frame(item: &mut ImageCacheItem, window: &mut Window, cx: &mut App) {
    if let Some(Ok(image)) = item.get() {
        cx.drop_image(image, Some(window));
    }
}

impl ImageCache for Covers {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        // `get_mut` is the touch: it is what moves a cover back to the head of
        // the recency list, so the one on screen is never the one evicted.
        if let Some(item) = self.0.get_mut(&key) {
            return item.get();
        }

        let load = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let task = cx.background_executor().spawn(load).shared();
        if let Some((_, mut evicted)) = self.0.push(key, ImageCacheItem::Loading(task.clone())) {
            drop_frame(&mut evicted, window, cx);
        }

        let view = window.current_view();
        window
            .spawn(cx, async move |cx| {
                let _ = task.await;
                cx.on_next_frame(move |_, cx| cx.notify(view));
            })
            .detach();
        None
    }
}

/// Install the caches. Called once, beside the other `init`s.
pub fn init(limit: u64, cx: &mut App) {
    let covers = cx.new(|_| Covers(LruCache::new(capacity(limit))));
    cx.set_global(Cache { covers });
}

pub fn covers(cx: &App) -> Entity<Covers> {
    cx.global::<Cache>().covers.clone()
}
