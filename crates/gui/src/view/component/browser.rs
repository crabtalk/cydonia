//! Browser tabs in the right panel.
//!
//! A tab holds a tab id, not the page. Pages live in [`Pages`], app-wide, so a
//! panel or window dropping does not drop them; only closing the tab does.

use bezel::{
    gpui::{
        self, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Global, KeyBinding,
        Subscription, Window, actions, div, prelude::*, px,
    },
    motion::{Fade, Painter},
    theme::{TextStyle, Theme, Typeset},
    ui::{
        icons,
        input::TextField,
        tooltip::Tooltip,
        widgets::{ButtonStyle, Buttons as _},
    },
};
use browser::{WebView, WebViewEvent};
use std::collections::HashMap;

actions!(cydonia_browser, [Go]);

/// Claimed on the address field, so `enter` loads what it holds.
const ADDRESS_CONTEXT: &str = "CydoniaAddress";

/// What a new tab opens on.
pub const HOME: &str = "https://duckduckgo.com";

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("enter", Go, Some(ADDRESS_CONTEXT))]
}

/// Whether pages can be shown here. bezel-browser builds pages under X11
/// only, so a gpui window on Wayland shows none.
pub fn supported(cx: &App) -> bool {
    !(cfg!(target_os = "linux") && cx.compositor_name() == "Wayland")
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

/// The title or location changed.
pub struct Changed;

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
            let page = cx.new(|cx| WebView::new(url, window, cx));
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
            WebViewEvent::Load(_) => {}
        }
        cx.emit(Changed);
        cx.notify();
    }

    fn go(&mut self, _: &Go, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.address.read(cx).content().trim().to_owned();
        if typed.is_empty() {
            return;
        }
        let url = address(&typed);
        let page = self.page(window, cx);
        page.update(cx, |page, _| page.load(url));
        window.focus(&page.focus_handle(cx), cx);
    }
}

/// What the address field's text loads: a URL as typed, a bare host over
/// https, anything else as a search.
fn address(typed: &str) -> String {
    if typed.contains("://") || typed.starts_with("about:") {
        typed.to_owned()
    } else if !typed.contains(char::is_whitespace) && typed.contains('.') {
        format!("https://{typed}")
    } else {
        let query: String = url::form_urlencoded::byte_serialize(typed.as_bytes()).collect();
        format!("{HOME}/?q={query}")
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
        if !supported(cx) {
            let theme = Theme::of(cx);
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .track_focus(&self.focus)
                .text_style(TextStyle::Caption)
                .text_color(theme.text_muted)
                .child("The browser needs X11. This session runs on Wayland.")
                .into_any_element();
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
                    ),
            )
            .child(div().flex_1().min_h_0().child(page))
            .into_any_element()
    }
}
