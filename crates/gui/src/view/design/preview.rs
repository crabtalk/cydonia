use crate::view::kit::web;
use artifact::{canvas::web::Source, design::Page, web::Viewport};
use bezel::gpui::{
    App, AppContext as _, AsyncApp, Entity, Pixels, RenderImage, Size, Window, px, size,
};
use browser::WebView;
use std::{path::Path, sync::Arc, time::Duration};

const WAIT: Duration = Duration::from_secs(12);
const THUMBNAIL_EDGE: u32 = 512;
const STEP: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Revision {
    pub(super) page: String,
    revision: String,
    viewport: (u32, u32),
}

impl Revision {
    pub(super) fn new(page: &Page, viewport: Viewport) -> Option<Self> {
        Some(Self {
            page: page.id.clone(),
            revision: page.revision.clone()?,
            viewport: (viewport.width, viewport.height),
        })
    }
}

pub(super) fn open(
    folder: &Path,
    page: &Page,
    viewport: Viewport,
    interactive: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<Entity<WebView>, String> {
    let path = folder.join(page.path());
    let directory = path.parent().ok_or("Page directory is missing")?;
    if !directory.starts_with(folder) || page.revision.is_none() {
        return Err("Waiting for index.html".into());
    }
    let url = Source::try_from(page.path())?.resolve(folder)?.url;
    let browser = cx.new(|cx| {
        WebView::new(url, window, cx)
            .with_read_access(directory.to_path_buf())
            .with_console()
    });
    web::present(&browser, viewport, interactive, window, cx)?;
    Ok(browser)
}

pub(super) async fn snapshot(
    browser: &Entity<WebView>,
    cx: &mut AsyncApp,
) -> Result<Arc<RenderImage>, String> {
    let mut waited = Duration::ZERO;
    loop {
        let ready = browser.read_with(cx, |browser, _| {
            if let Some(error) = &browser.error {
                return Err(error.clone());
            }
            Ok(browser.can_evaluate() && !browser.is_loading())
        })?;
        if ready {
            let image = browser
                .read_with(cx, |browser, cx| browser.snapshot(cx))
                .await?;
            return cx
                .background_executor()
                .spawn(async move {
                    let size = image.size(0);
                    let buffer = image::RgbaImage::from_raw(
                        size.width.0 as u32,
                        size.height.0 as u32,
                        image.as_bytes(0).ok_or("Page snapshot is empty")?.to_vec(),
                    )
                    .ok_or("Page snapshot has invalid dimensions")?;
                    let thumbnail = image::DynamicImage::ImageRgba8(buffer)
                        .thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE)
                        .into_rgba8();
                    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(
                        thumbnail,
                    )])))
                })
                .await;
        }
        if waited >= WAIT {
            return Err("Page preview timed out".into());
        }
        cx.background_executor().timer(STEP).await;
        waited += STEP;
    }
}

pub(super) fn fit(viewport: Viewport, available: Size<Pixels>) -> Size<Pixels> {
    let scale = (available.width.as_f32() / viewport.width as f32)
        .min(available.height.as_f32() / viewport.height as f32)
        .min(1.)
        .max(0.);
    size(
        px(viewport.width as f32 * scale),
        px(viewport.height as f32 * scale),
    )
}
