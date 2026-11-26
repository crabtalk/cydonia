use crate::view::{
    component::browse::console::ConsoleLog,
    kit::web::{Navigation, WebPresentation},
};
use artifact::{canvas::web::Source, design::Artboard, web::Viewport};
use bezel::gpui::{
    App, AppContext as _, AsyncApp, Entity, Pixels, RenderImage, Size, Subscription, Window, div,
    prelude::*, px, size,
};
use browser::{WebView, WebViewEvent};
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc, time::Duration};

const WAIT: Duration = Duration::from_secs(12);
pub(super) const THUMBNAIL_WIDTH: u32 = 960;
const MAX_PREVIEW_PIXELS: f64 = 12_000_000.;
const PREVIEW_HEIGHT: u32 = 8192;
const DETAIL_STEP: u32 = 256;
const EVAL: Duration = Duration::from_secs(2);
const STEP: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Revision {
    pub(super) artboard: String,
    revision: String,
    viewport: (u32, u32),
}

impl Revision {
    pub(super) fn new(artboard: &Artboard, viewport: Viewport) -> Option<Self> {
        Some(Self {
            artboard: artboard.id.clone(),
            revision: artboard.revision.clone()?,
            viewport: (viewport.width, viewport.height),
        })
    }
}

#[derive(Clone)]
pub(crate) struct LiveArtboard {
    pub(crate) browser: Entity<WebView>,
    pub(crate) console: Rc<RefCell<ConsoleLog>>,
    navigation: Navigation,
    _events: Rc<Subscription>,
}

pub(crate) fn host(browser: Entity<WebView>) -> impl IntoElement {
    div()
        .absolute()
        .right_0()
        .bottom_0()
        .size(px(1.))
        .opacity(0.01)
        .child(browser)
}

pub(crate) fn open(
    folder: &Path,
    artboard: &Artboard,
    viewport: Viewport,
    interactive: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<LiveArtboard, String> {
    let path = folder.join(artboard.path());
    let directory = path.parent().ok_or("Artboard directory is missing")?;
    if !directory.starts_with(folder) || !path.is_file() {
        return Err("Waiting for index.html".into());
    }
    let url = Source::try_from(artboard.path())?.resolve(folder)?.url;
    let browser = cx.new(|cx| {
        let page = WebView::new(url, window, cx)
            .with_read_access(directory.to_path_buf())
            .with_console();
        if interactive {
            page.with_pinch_zoom()
        } else {
            page
        }
    });
    browser.present(viewport, interactive, window, cx)?;
    let console = Rc::new(RefCell::new(ConsoleLog::default()));
    let log = console.clone();
    let navigation = Navigation::default();
    let load = navigation.clone();
    let events = cx.subscribe(
        &browser,
        move |browser, event: &WebViewEvent, cx| match event {
            WebViewEvent::Console(message) => log.borrow_mut().push(message.clone()),
            WebViewEvent::Load(state) => load.update(*state, browser.read(cx)),
            _ => {}
        },
    );
    Ok(LiveArtboard {
        browser,
        console,
        navigation,
        _events: Rc::new(events),
    })
}

impl LiveArtboard {
    pub(super) fn is_ready(&self, cx: &App) -> bool {
        let browser = self.browser.read(cx);
        self.navigation.ready(browser)
    }

    pub(crate) async fn ready(&self, cx: &mut AsyncApp) -> Result<(), String> {
        let mut waited = Duration::ZERO;
        loop {
            let ready = self.browser.read_with(cx, |browser, _| {
                if let Some(error) = &browser.error {
                    return Err(error.clone());
                }
                Ok(self.navigation.ready(browser))
            })?;
            if ready {
                return Ok(());
            }
            if waited >= WAIT {
                return Err("Artboard preview timed out".into());
            }
            cx.background_executor().timer(STEP).await;
            waited += STEP;
        }
    }

    pub(super) async fn preview(
        &self,
        viewport: Viewport,
        requested_width: u32,
        cx: &mut AsyncApp,
    ) -> Result<Preview, String> {
        self.ready(cx).await?;
        let height = self
            .browser
            .read_with(cx, |browser, cx| {
                browser.eval::<u32>(
                    "Math.max(document.documentElement.scrollHeight, innerHeight)",
                    EVAL,
                    cx,
                )
            })
            .await
            .map_err(|error| error.to_string())?;
        let mut assembled = None;
        let mut top = 0;
        while top < height {
            // Keep the configured viewport so height media queries and vh units
            // resolve the same way here and in the interactive artboard.
            let scroll = format!(
                "(() => {{
                    document.documentElement.style.setProperty('scroll-snap-type', 'none', 'important');
                    window.scrollTo({{ top: {top}, behavior: 'instant' }});
                    return Math.round(window.scrollY);
                }})()",
            );
            let offset = self
                .browser
                .read_with(cx, |browser, cx| browser.eval::<u32>(&scroll, EVAL, cx))
                .await
                .map_err(|error| error.to_string())?;
            let bottom = offset.saturating_add(viewport.height).min(height);
            if offset > top || bottom <= top {
                return Err("Could not scroll to the end of the artboard".into());
            }
            let screenshot = self
                .browser
                .read_with(cx, |browser, cx| browser.snapshot(cx))
                .await?;
            let width = assembled.as_ref().map_or_else(
                || {
                    Preview::width_limit(
                        viewport,
                        height,
                        requested_width.min(screenshot.size(0).width.0 as u32),
                    )
                },
                image::RgbaImage::width,
            );
            let scaled = |length| {
                (u64::from(length) * u64::from(width) / u64::from(viewport.width))
                    .min(u64::from(PREVIEW_HEIGHT)) as u32
            };
            let target_y = scaled(top);
            let target_rows = scaled(bottom)
                .max(u32::from(bottom == height))
                .saturating_sub(target_y);
            if target_rows == 0 {
                top = bottom;
                continue;
            }
            let output_height = scaled(height).max(1);
            let to_px = move |length, rows| {
                (u64::from(length) * u64::from(rows) / u64::from(viewport.height)) as u32
            };
            assembled = Some(
                cx.background_executor()
                    .spawn(async move {
                        let size = screenshot.size(0);
                        let (columns, rows) = (size.width.0 as u32, size.height.0 as u32);
                        let strip = image::RgbaImage::from_raw(
                            columns,
                            rows,
                            screenshot
                                .as_bytes(0)
                                .ok_or("Artboard snapshot is empty")?
                                .to_vec(),
                        )
                        .ok_or("Artboard snapshot has invalid dimensions")?;
                        let start = to_px(top - offset, rows);
                        let end = to_px(bottom - offset, rows);
                        let crop = image::imageops::crop_imm(
                            &strip,
                            0,
                            start,
                            columns,
                            (end - start).max(1),
                        );
                        let mut assembled = assembled
                            .unwrap_or_else(|| image::RgbaImage::new(width, output_height));
                        let resized = image::imageops::thumbnail(&*crop, width, target_rows);
                        image::imageops::replace(&mut assembled, &resized, 0, i64::from(target_y));
                        Ok::<_, String>(assembled)
                    })
                    .await?,
            );
            top = bottom;
        }
        Ok(Preview {
            height,
            image: Arc::new(RenderImage::new(vec![image::Frame::new(
                assembled.ok_or("Artboard snapshot is empty")?,
            )])),
        })
    }
}

pub(super) struct Preview {
    pub(super) height: u32,
    pub(super) image: Arc<RenderImage>,
}

impl Preview {
    pub(super) fn detail_width(&self, viewport: Viewport, zoom: f32, scale_factor: f32) -> u32 {
        let native_width = viewport.width as f32 * scale_factor;
        let wanted = (native_width * zoom).ceil() as u32;
        let bucket = wanted.div_ceil(DETAIL_STEP).saturating_mul(DETAIL_STEP);
        Self::width_limit(
            viewport,
            self.height,
            bucket.min(native_width.ceil() as u32),
        )
    }

    fn width_limit(viewport: Viewport, height: u32, requested_width: u32) -> u32 {
        let ratio = f64::from(viewport.width) / f64::from(height.max(1));
        requested_width
            .min((ratio * f64::from(PREVIEW_HEIGHT)) as u32)
            .min((ratio * MAX_PREVIEW_PIXELS).sqrt() as u32)
            .max(1)
    }
}

pub(super) fn fit(viewport: Viewport, available: Size<Pixels>) -> Size<Pixels> {
    let scale = (available.width.as_f32() / viewport.width as f32)
        .min(available.height.as_f32() / viewport.height as f32)
        .clamp(0., 1.);
    size(
        px(viewport.width as f32 * scale),
        px(viewport.height as f32 * scale),
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/design_preview.rs"]
mod tests;
