//! Browser tabs in the right panel.
//!
//! A tab holds a tab id, not the page. Pages live in [`Pages`], app-wide, so a
//! panel or window dropping does not drop them; only closing the tab does.

use crate::model::settings::Browsing;
use bezel::{
    gpui::{
        self, AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
        Global, KeyBinding, MouseButton, MouseDownEvent, Pixels, Point, Subscription, Window,
        actions, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::Theme,
    ui::{
        context_menu::{ContextMenu, Place},
        icons,
        input::TextField,
        menu::Item,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use browser::{ConsoleMessage, DataStore, WebView, WebViewEvent};
use std::collections::{HashMap, VecDeque};

/// Claimed on the address field, so `enter` loads what it holds.
const ADDRESS_CONTEXT: &str = "CydoniaAddress";

/// Claimed on the whole tab, the address field included.
const CONTEXT: &str = "CydoniaBrowser";

/// Messages a tab's log keeps; older ones are dropped.
const LOG_LIMIT: usize = 500;

actions!(cydonia_browser, [Go, Reload, HardReload]);

/// What a new tab opens on: the home page in Settings.
pub fn home(cx: &App) -> String {
    cx.try_global::<Browsing>().map_or_else(
        || Browsing::default().home,
        |browsing| browsing.home.clone(),
    )
}

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", Go, Some(ADDRESS_CONTEXT)),
        KeyBinding::new("secondary-r", Reload, Some(CONTEXT)),
        KeyBinding::new("f5", Reload, Some(CONTEXT)),
        KeyBinding::new("secondary-shift-r", HardReload, Some(CONTEXT)),
    ]
}

/// Every live page, by tab id.
#[derive(Default)]
struct Pages(HashMap<u64, Entity<WebView>>);

impl Global for Pages {}

/// What each live page has logged, by tab id, oldest first.
#[derive(Default)]
struct Logs(HashMap<u64, VecDeque<ConsoleMessage>>);

impl Global for Logs {}

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
    if let Some(logs) = cx.try_global::<Logs>()
        && logs.0.contains_key(&id)
    {
        cx.global_mut::<Logs>().0.remove(&id);
    }
}

#[derive(Default)]
struct Clearing(bool);
impl Global for Clearing {}

pub(crate) fn clearing(cx: &App) -> bool {
    cx.try_global::<Clearing>().is_some_and(|state| state.0)
}

pub(crate) fn set_clearing(value: bool, cx: &mut App) {
    cx.default_global::<Clearing>().0 = value;
    cx.refresh_windows();
}

pub(crate) fn forget_all(cx: &mut App) {
    cx.default_global::<Pages>().0.clear();
    cx.default_global::<Logs>().0.clear();
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
    /// The reload button's right-press menu.
    menu: ContextMenu<Self>,
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
            menu: ContextMenu::default(),
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
    /// What the tab's page has logged, oldest first.
    pub(crate) fn console(&self, cx: &App) -> Vec<ConsoleMessage> {
        cx.try_global::<Logs>()
            .and_then(|logs| logs.0.get(&self.id))
            .map(|log| log.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn webview(&self) -> Option<Entity<WebView>> {
        self.page.clone()
    }

    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        self._page = None;
        self.page = None;
        forget(self.id, cx);
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
            let page = cx.new(|cx| {
                WebView::new(url, window, cx)
                    .with_data_store(store)
                    .with_console()
                    .with_inspector()
            });
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
            WebViewEvent::Load(_) | WebViewEvent::History { .. } => {}
            WebViewEvent::Console(message) => {
                let log = cx.default_global::<Logs>().0.entry(self.id).or_default();
                if log.len() == LOG_LIMIT {
                    log.pop_front();
                }
                log.push_back(message.clone());
                return;
            }
            WebViewEvent::NewWindow(url) => {
                cx.emit(OpenTab(url.clone()));
                return;
            }
        }
        cx.emit(Changed);
        cx.notify();
    }

    fn reload(&mut self, _: &Reload, window: &mut Window, cx: &mut Context<Self>) {
        let page = self.page(window, cx);
        page.update(cx, |page, _| page.reload());
    }

    /// Reload with nothing taken from the cache.
    fn hard_reload(&mut self, _: &HardReload, window: &mut Window, cx: &mut Context<Self>) {
        let page = self.page(window, cx);
        page.update(cx, |page, _| page.reload_bypassing_cache());
    }

    /// Open the reload button's menu at `at`.
    fn open_reload_menu(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let items = vec![
            Item::action("Reload").with_shortcut_in(&Reload, CONTEXT, window),
            Item::action("Hard Reload").with_shortcut_in(&HardReload, CONTEXT, window),
            Item::action("Web Inspector"),
        ];
        self.menu.open(
            (),
            Place::At(at),
            items,
            |this, path, window, cx| match path.first() {
                Some(0) => this.reload(&Reload, window, cx),
                Some(1) => this.hard_reload(&HardReload, window, cx),
                Some(2) => this.page(window, cx).read(cx).open_inspector(),
                _ => {}
            },
            window,
            cx,
        );
    }

    /// The reload button's menu, while it is open.
    fn reload_menu(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.menu
            .render("browser-reload-menu", |this| &mut this.menu, window, cx)
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
        if clearing(cx) {
            return div().into_any_element();
        }
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
        let (can_back, can_forward) = {
            let page = page.read(cx);
            (page.can_go_back(), page.can_go_forward())
        };
        // Nowhere to go: faint, no hover, no press.
        let stuck = |icon: &'static [u8], id: &'static str| {
            theme
                .tinted_icon_button(icon, theme.text_faint)
                .id(id)
                .flex_none()
                .into_any_element()
        };
        let back = page.clone();
        let forward = page.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::hard_reload))
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
                    .child(match can_back {
                        true => nav(icons::arrows::ArrowLeft, "browser-back", "Back")
                            .on_click(move |_, _, cx| back.update(cx, |page, _| page.back()))
                            .into_any_element(),
                        false => stuck(icons::arrows::ArrowLeft, "browser-back"),
                    })
                    .child(match can_forward {
                        true => nav(icons::arrows::ArrowRight, "browser-forward", "Forward")
                            .on_click(move |_, _, cx| forward.update(cx, |page, _| page.forward()))
                            .into_any_element(),
                        false => stuck(icons::arrows::ArrowRight, "browser-forward"),
                    })
                    .child(
                        nav(
                            icons::arrows::RefreshCw,
                            "browser-reload",
                            if loading {
                                "Loading…"
                            } else {
                                "Reload — ⇧-click or right-click for Hard Reload"
                            },
                        )
                        .on_click(cx.listener(|this, click: &ClickEvent, window, cx| {
                            match click.modifiers().shift {
                                true => this.hard_reload(&HardReload, window, cx),
                                false => this.reload(&Reload, window, cx),
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, press: &MouseDownEvent, window, cx| {
                                this.open_reload_menu(press.position, window, cx);
                                cx.stop_propagation();
                            }),
                        ),
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
            .children(self.reload_menu(window, cx))
            .into_any_element()
    }
}
