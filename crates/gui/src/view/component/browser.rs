//! Browser tabs in the right panel.
//!
//! A tab holds a tab id, not the page. Pages live in [`Pages`], app-wide, so a
//! panel or window dropping does not drop them; only closing the tab does.

use crate::{
    model::settings::{Browsing, Links},
    view::root::Cydonia,
};
use bezel::{
    gpui::{
        self, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Global, KeyBinding,
        Subscription, Window, actions, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        icons,
        input::TextField,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use browser::{DataStore, WebView, WebViewEvent};
use std::collections::HashMap;

actions!(cydonia_browser, [Go]);

/// Claimed on the address field, so `enter` loads what it holds.
const ADDRESS_CONTEXT: &str = "CydoniaAddress";

/// What a new tab opens on: the home page in Settings.
pub fn home(cx: &App) -> String {
    cx.try_global::<Browsing>().map_or_else(
        || Browsing::default().home,
        |browsing| browsing.home.clone(),
    )
}

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("enter", Go, Some(ADDRESS_CONTEXT))]
}

/// Every live page, by tab id.
#[derive(Default)]
struct Pages(HashMap<u64, Entity<WebView>>);

impl Global for Pages {}

/// A tab id no other tab holds: tab ids are saved across restarts.
pub fn new_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}

/// Drop the tab's page. Called on closing the tab.
pub fn forget(id: u64, cx: &mut App) {
    if let Some(pages) = cx.try_global::<Pages>()
        && pages.0.contains_key(&id)
    {
        cx.global_mut::<Pages>().0.remove(&id);
    }
}

/// Opens a link clicked in an article or a transcript: an http(s) link in a
/// panel tab where Settings says so and the window can take one, else in the
/// system browser. Installed as markdown's link handler.
pub fn open_link(url: &str, window: &mut Window, cx: &mut App) {
    let panel = cx
        .try_global::<Browsing>()
        .is_some_and(|browsing| browsing.links == Links::Panel);
    let web = url.starts_with("https://") || url.starts_with("http://");
    if panel
        && web
        && let Some(Some(root)) = window.root::<Cydonia>()
        && root.update(cx, |root, cx| {
            root.open_in_panel(url.to_owned(), window, cx)
        })
    {
        return;
    }
    cx.open_url(url);
}

/// Whether any page is built, which is what [`clear_data`] clears through.
pub fn any_page(cx: &App) -> bool {
    cx.try_global::<Pages>()
        .is_some_and(|pages| !pages.0.is_empty())
}

/// Clear the cookies, storage and cache of every live page's store. `false`
/// where no page could clear.
pub fn clear_data(cx: &App) -> bool {
    let Some(pages) = cx.try_global::<Pages>() else {
        return false;
    };
    // Every page, not the first that answers: an in-memory page has a store
    // of its own.
    let cleared = pages
        .0
        .values()
        .filter(|page| page.read(cx).clear_data())
        .count();
    cleared > 0
}

/// The title or location changed.
pub struct Changed;

/// The page asked for a new window — a `target="_blank"` link or
/// `window.open` — on this URL.
pub struct OpenTab(pub String);

pub struct Browser {
    pub id: u64,
    /// Built on first render: a page needs a window.
    page: Option<Entity<WebView>>,
    /// The page's last location, or the saved one until it is built.
    url: String,
    pub(super) title: String,
    address: Entity<TextField>,
    focus: FocusHandle,
    _page: Option<Subscription>,
}

impl EventEmitter<Changed> for Browser {}
impl EventEmitter<OpenTab> for Browser {}

impl Browser {
    pub fn new(id: u64, url: String, title: String, cx: &mut Context<Self>) -> Self {
        let address = cx.new(|cx| {
            TextField::new(cx)
                .with_key_context(ADDRESS_CONTEXT)
                .with_placeholder("Search or enter address")
        });
        address.update(cx, |field, cx| field.set_content(url.clone(), cx));
        Self {
            id,
            page: None,
            url,
            title,
            address,
            focus: cx.focus_handle(),
            _page: None,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// The page's title, or its location while it has none.
    pub fn title(&self) -> &str {
        if self.title.is_empty() {
            &self.url
        } else {
            &self.title
        }
    }

    /// The tab's page, once a render has built it.
    pub(crate) fn webview(&self) -> Option<Entity<WebView>> {
        self.page.clone()
    }

    /// Load `url`: in the page where it is built, else the page is built on it.
    pub(crate) fn navigate(&mut self, url: String, cx: &mut Context<Self>) {
        match &self.page {
            Some(page) => page.update(cx, |page, _| page.load(url)),
            None => {
                self.address
                    .update(cx, |field, cx| field.set_content(url.clone(), cx));
                self.url = url;
            }
        }
        cx.notify();
    }

    pub fn address_focus(&self, cx: &App) -> FocusHandle {
        self.address.focus_handle(cx)
    }

    fn page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<WebView> {
        if let Some(page) = &self.page {
            return page.clone();
        }
        let existing = cx
            .try_global::<Pages>()
            .and_then(|pages| pages.0.get(&self.id).cloned());
        let page = existing.unwrap_or_else(|| {
            let url = self.url.clone();
            let keep = cx
                .try_global::<Browsing>()
                .is_none_or(|browsing| browsing.keep_signed_in);
            let store = match keep {
                true => DataStore::new(),
                false => DataStore::new().incognito(),
            };
            let page = cx.new(|cx| WebView::new(url, window, cx).with_data_store(store));
            cx.default_global::<Pages>().0.insert(self.id, page.clone());
            page
        });
        let current = page.read(cx);
        if let Some(location) = current.location() {
            self.url = location.to_owned();
        }
        if !current.title().is_empty() {
            self.title = current.title().to_owned();
        }
        self._page = Some(cx.subscribe_in(&page, window, Self::report));
        self.page = Some(page.clone());
        page
    }

    fn report(
        &mut self,
        _: &Entity<WebView>,
        event: &WebViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            WebViewEvent::Location(url) => {
                self.url = url.clone();
                if !self.address.focus_handle(cx).is_focused(window) {
                    self.address
                        .update(cx, |field, cx| field.set_content(url.clone(), cx));
                }
            }
            WebViewEvent::Title(title) => self.title = title.clone(),
            WebViewEvent::Load(_)
            | WebViewEvent::DownloadStarted { .. }
            | WebViewEvent::DownloadFinished { .. } => {}
            WebViewEvent::NewWindow(url) => {
                cx.emit(OpenTab(url.clone()));
                return;
            }
        }
        cx.emit(Changed);
        cx.notify();
    }

    fn go(&mut self, _: &Go, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.address.read(cx).content().trim().to_owned();
        if typed.is_empty() {
            return;
        }
        let url = address(&typed, cx);
        let page = self.page(window, cx);
        page.update(cx, |page, _| page.load(url));
        window.focus(&page.focus_handle(cx), cx);
    }
}

/// What the address field's text loads: a URL as typed, a bare host over
/// https, anything else as a search with the engine in Settings.
fn address(typed: &str, cx: &App) -> String {
    if typed.contains("://") || typed.starts_with("about:") {
        typed.to_owned()
    } else if !typed.contains(char::is_whitespace) && typed.contains('.') {
        format!("https://{typed}")
    } else {
        cx.try_global::<Browsing>()
            .cloned()
            .unwrap_or_default()
            .search_url(typed)
    }
}

impl Focusable for Browser {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.page {
            Some(page) => page.focus_handle(cx),
            None => self.focus.clone(),
        }
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page(window, cx);
        let theme = Theme::of(cx).clone();
        let loading = page.read(cx).is_loading();
        let nav = |icon: &'static [u8], id: &'static str, tip: &'static str| {
            theme
                .icon_button(
                    icon,
                    ButtonStyle::Ghost,
                    Some(Fade::new(Painter::of(cx), id)),
                )
                .id(id)
                .flex_none()
                .tooltip(move |window, cx| Tooltip::text(tip, window, cx))
        };
        let back = page.clone();
        let forward = page.clone();
        let reload = page.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.))
                    .px(px(6.))
                    .py(px(4.))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        nav(icons::arrows::ArrowLeft, "browser-back", "Back")
                            .on_click(move |_, _, cx| back.update(cx, |page, _| page.back())),
                    )
                    .child(
                        nav(icons::arrows::ArrowRight, "browser-forward", "Forward")
                            .on_click(move |_, _, cx| forward.update(cx, |page, _| page.forward())),
                    )
                    .child(
                        nav(
                            icons::arrows::RefreshCw,
                            "browser-reload",
                            if loading { "Loading…" } else { "Reload" },
                        )
                        .on_click(move |_, _, cx| reload.update(cx, |page, _| page.reload())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .on_action(cx.listener(Self::go))
                            .child(self.address.clone()),
                    )
                    .child(
                        nav(
                            icons::arrows::ExternalLink,
                            "browser-system",
                            "Open in system browser",
                        )
                        .on_click(cx.listener(|this, _, _, cx| cx.open_url(&this.url))),
                    ),
            )
            .child(div().flex_1().min_h_0().child(page))
            .into_any_element()
    }
}
