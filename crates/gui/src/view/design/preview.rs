use crate::view::{component::browse::console::ConsoleLog, kit::web};
use artifact::{canvas::web::Source, design::Page, web::Viewport};
use bezel::gpui::{
    App, AppContext as _, AsyncApp, Entity, Pixels, RenderImage, Size, Subscription, Window, div,
    prelude::*, px, size,
};
use browser::{WebView, WebViewEvent};
use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc, time::Duration};

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

#[derive(Clone)]
pub(crate) struct LivePage {
    pub(crate) browser: Entity<WebView>,
    pub(crate) console: Rc<RefCell<ConsoleLog>>,
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
    page: &Page,
    viewport: Viewport,
    interactive: bool,
    window: &mut Window,
    cx: &mut App,
) -> Result<LivePage, String> {
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
    let console = Rc::new(RefCell::new(ConsoleLog::default()));
    let log = console.clone();
    let events = cx.subscribe(&browser, move |_, event: &WebViewEvent, _| {
        if let WebViewEvent::Console(message) = event {
            log.borrow_mut().push(message.clone());
        }
    });
    Ok(LivePage {
        browser,
        console,
        _events: Rc::new(events),
    })
}

pub(crate) async fn ready(browser: &Entity<WebView>, cx: &mut AsyncApp) -> Result<(), String> {
    let mut waited = Duration::ZERO;
    loop {
        let ready = browser.read_with(cx, |browser, _| {
            if let Some(error) = &browser.error {
                return Err(error.clone());
            }
            Ok(browser.can_evaluate() && !browser.is_loading())
        })?;
        if ready {
            return Ok(());
        }
        if waited >= WAIT {
            return Err("Page preview timed out".into());
        }
        cx.background_executor().timer(STEP).await;
        waited += STEP;
    }
}

pub(super) async fn snapshot(
    browser: &Entity<WebView>,
    cx: &mut AsyncApp,
) -> Result<Arc<RenderImage>, String> {
    ready(browser, cx).await?;
    let image = browser
        .read_with(cx, |browser, cx| browser.snapshot(cx))
        .await?;
    cx.background_executor()
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
        .await
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
