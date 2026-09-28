//! What an agent does in a project's browser tabs: open a page, read it, act
//! on it — the window's half of [`mcp::tools::browser`].
//!
//! Elements are addressed by the number a read gave them. Every script derives
//! the list with [`FIND`], so a number from a read names the same element when
//! the next call derives the list again.
//!
//! Scripts run through [`WebView::eval`], which exists only once a render has
//! built the page: a tab whose panel is not on screen has none.

use super::{browser::Browser, panel::Panel};
use crate::view::root::Cydonia;
use bezel::gpui::{AsyncApp, Context, Entity, WeakEntity};
use browser::WebView;
use futures::{StreamExt as _, channel::mpsc};
use mcp::rail::{self, Act, Browse};
use serde::{Deserialize, de::DeserializeOwned};
use std::{fmt::Write as _, path::Path, time::Duration};

/// Characters of a page's text a read carries. Sliced in the page, so the rest
/// never crosses the bridge.
const MAX_TEXT: usize = 8000;

/// Elements listed per read.
const MAX_ELEMENTS: usize = 200;

/// How long one script may run.
const EVAL: Duration = Duration::from_secs(10);

/// How long a tab is given to build its page after it is opened.
const BUILT: Duration = Duration::from_secs(3);

/// How long a page is given to finish loading after an action.
const SETTLE: Duration = Duration::from_secs(10);

const STEP: Duration = Duration::from_millis(100);

#[derive(Deserialize)]
struct Page {
    url: String,
    title: String,
    text: String,
    #[serde(default)]
    truncated: bool,
    elements: Vec<Element>,
}

#[derive(Deserialize)]
struct Element {
    i: usize,
    tag: String,
    kind: Option<String>,
    name: String,
}

const FIND: &str = r#"const found = (() => {
  const pick = 'a[href], button, input, select, textarea, summary, [role=button], [role=link], [role=tab], [role=checkbox], [role=menuitem], [role=textbox], [contenteditable=""], [contenteditable=true], [onclick]';
  const out = [];
  for (const el of document.querySelectorAll(pick)) {
    if (out.length >= $MAX_ELEMENTS) break;
    if (el.disabled) continue;
    const box = el.getBoundingClientRect();
    if (box.width < 1 || box.height < 1) continue;
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.opacity === '0') continue;
    out.push(el);
  }
  return out;
})();"#;

const SNAPSHOT: &str = r#"(() => { try {
  $FIND
  const listed = found.map((el, i) => {
    const tail = el.tagName === 'A' ? (el.getAttribute('href') || '').split(/[?#]/)[0].split('/').filter(Boolean).pop() : '';
    const name = (el.getAttribute('aria-label') || el.placeholder || el.innerText || el.value || el.alt || el.title || tail || '').replace(/\s+/g, ' ').trim().slice(0, 100);
    return { i, tag: el.tagName.toLowerCase(), kind: el.type || el.getAttribute('role') || null, name };
  });
  const text = (document.body ? document.body.innerText : '').replace(/\n\s*\n\s*\n+/g, '\n\n').trim();
  return { url: location.href, title: document.title, text: text.slice(0, $MAX_TEXT), truncated: text.length > $MAX_TEXT, elements: listed };
} catch (e) { return { url: location.href, title: document.title, text: String(e), elements: [] }; } })()"#;

const CLICK: &str = r#"(() => { try {
  $FIND
  const el = found[$N];
  if (!el) return 'gone';
  el.scrollIntoView({ block: 'center' });
  el.click();
  return 'ok';
} catch (e) { return String(e); } })()"#;

const FILL: &str = r#"(() => { try {
  $FIND
  const el = found[$N];
  if (!el) return 'gone';
  el.scrollIntoView({ block: 'center' });
  el.focus();
  const text = $TEXT;
  if (el.isContentEditable) el.textContent = text;
  else {
    // A framework's input tracks its value through the prototype's setter and
    // ignores a plain assignment.
    const own = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
    if (own && own.set) own.set.call(el, text); else el.value = text;
  }
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  if ($ENTER) {
    const key = { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true };
    // A field that handled Return itself has submitted already.
    const handled = !el.dispatchEvent(new KeyboardEvent('keydown', key));
    el.dispatchEvent(new KeyboardEvent('keypress', key));
    el.dispatchEvent(new KeyboardEvent('keyup', key));
    if (!handled && el.form && el.form.requestSubmit) el.form.requestSubmit();
  }
  return 'ok';
} catch (e) { return String(e); } })()"#;

const SCROLL: &str = r#"(() => { try {
  const page = document.scrollingElement || document.documentElement;
  const was = page.scrollTop;
  page.scrollTop += window.innerHeight * $PAGES;
  // An app-shell layout scrolls a container rather than the document.
  if (page.scrollTop === was) {
    const inner = [...document.querySelectorAll('*')]
      .filter((el) => el.scrollHeight > el.clientHeight + 20 && /auto|scroll/.test(getComputedStyle(el).overflowY))
      .sort((a, b) => b.clientHeight - a.clientHeight)[0];
    if (inner) inner.scrollTop += inner.clientHeight * $PAGES;
  }
  return 'ok';
} catch (e) { return String(e); } })()"#;

fn script(js: &str) -> String {
    js.replace("$FIND", FIND)
        .replace("$MAX_ELEMENTS", &MAX_ELEMENTS.to_string())
        .replace("$MAX_TEXT", &MAX_TEXT.to_string())
}

impl Cydonia {
    /// Answer the browser tools, one call at a time. Called once, with the
    /// window.
    pub(crate) fn take_browser(&self, cx: &mut Context<Self>) {
        let (asked, mut asks) = mpsc::unbounded();
        rail::install_browser(move |browse| {
            let _ = asked.unbounded_send(browse);
        });
        cx.spawn(async move |this, cx| {
            while let Some(Browse {
                project,
                tab,
                act,
                reply,
            }) = asks.next().await
            {
                let answer = serve(&this, &project, tab, act, cx).await;
                let _ = reply.send(answer);
                if this.upgrade().is_none() {
                    return;
                }
            }
        })
        .detach();
    }

    /// The right panel of the open project at `path`.
    fn project_panel(&mut self, path: &Path, cx: &mut Context<Self>) -> Option<Entity<Panel>> {
        let settled = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let project = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .find(|open| {
                open.path == path || open.path.canonicalize().is_ok_and(|held| held == settled)
            })
            .map(|open| open.path.clone())?;
        Some(self.right_panel(project, cx))
    }
}

async fn serve(
    this: &WeakEntity<Cydonia>,
    project: &Path,
    tab: Option<u64>,
    act: Act,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    let panel = this
        .update(cx, |this, cx| this.project_panel(project, cx))
        .ok()
        .flatten()
        .ok_or_else(|| format!("cydonia does not have {} open", project.display()))?;
    match act {
        Act::Tabs => panel.update(cx, |panel, cx| Ok(tabs(panel, cx))),
        Act::Open(url) => {
            let browser = match tab {
                Some(_) => {
                    let browser = pick(&panel, tab, cx)?;
                    browser.update(cx, |browser, cx| browser.navigate(url, cx));
                    browser
                }
                None => panel
                    .update(cx, |panel, cx| panel.open_browser(url, cx))
                    .ok_or("browser tabs are switched off in cydonia's settings")?,
            };
            let page = built(&browser, cx).await?;
            settle(&page, cx).await;
            read(&browser, &page, cx).await
        }
        Act::Read => {
            let browser = pick(&panel, tab, cx)?;
            let page = built(&browser, cx).await?;
            read(&browser, &page, cx).await
        }
        Act::Click(element) => {
            let js = script(CLICK).replace("$N", &element.to_string());
            act_on(&panel, tab, &js, element, cx).await
        }
        Act::Type {
            element,
            text,
            enter,
        } => {
            let js = script(FILL)
                .replace("$N", &element.to_string())
                .replace(
                    "$TEXT",
                    &serde_json::to_string(&text).unwrap_or_else(|_| "''".into()),
                )
                .replace("$ENTER", if enter { "true" } else { "false" });
            act_on(&panel, tab, &js, element, cx).await
        }
        Act::Scroll(pages) => {
            let js = SCROLL.replace("$PAGES", &format!("{:.2}", pages.clamp(-20., 20.)));
            act_on(&panel, tab, &js, 0, cx).await
        }
    }
}

/// Run `js` in the tab and answer with the page it leaves.
async fn act_on(
    panel: &Entity<Panel>,
    tab: Option<u64>,
    js: &str,
    element: usize,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    let browser = pick(panel, tab, cx)?;
    let page = built(&browser, cx).await?;
    match eval::<String>(&page, js, cx).await?.as_str() {
        "ok" => {}
        "gone" => {
            return Err(format!(
                "element {element} is not on the page any more; read the tab for its current elements"
            ));
        }
        other => return Err(other.to_owned()),
    }
    // A click that navigates starts its load a moment after the script ends.
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    settle(&page, cx).await;
    read(&browser, &page, cx).await
}

/// The browser tab `tab` names, else the panel's front one, else its first.
fn pick(
    panel: &Entity<Panel>,
    tab: Option<u64>,
    cx: &mut AsyncApp,
) -> Result<Entity<Browser>, String> {
    let browsers = panel.read_with(cx, |panel, _| panel.browsers());
    let found = match tab {
        Some(id) => cx.update(|cx| {
            browsers
                .iter()
                .find(|(browser, _)| browser.read(cx).id == id)
                .map(|(browser, _)| browser.clone())
        }),
        None => browsers
            .iter()
            .find(|(_, front)| *front)
            .or(browsers.first())
            .map(|(browser, _)| browser.clone()),
    };
    found.ok_or_else(|| match tab {
        Some(id) => format!("no browser tab {id} in this project; browser_tabs lists them"),
        None => "no browser tab open in this project; browser_open opens one".to_owned(),
    })
}

/// The tab's page, waiting a moment for a render to build it.
async fn built(browser: &Entity<Browser>, cx: &mut AsyncApp) -> Result<Entity<WebView>, String> {
    let mut waited = Duration::ZERO;
    loop {
        if let Some(page) = browser.read_with(cx, |browser, _| browser.webview()) {
            return Ok(page);
        }
        if waited >= BUILT {
            return Err(
                "the tab is open in the project's right panel, but its page loads only \
                once that panel is on screen"
                    .to_owned(),
            );
        }
        cx.background_executor().timer(STEP).await;
        waited += STEP;
    }
}

/// Wait for the page to stop loading, up to [`SETTLE`].
async fn settle(page: &Entity<WebView>, cx: &mut AsyncApp) {
    let mut waited = Duration::ZERO;
    while waited < SETTLE && page.read_with(cx, |page, _| page.is_loading()) {
        cx.background_executor().timer(STEP).await;
        waited += STEP;
    }
}

async fn eval<T: DeserializeOwned + 'static>(
    page: &Entity<WebView>,
    js: &str,
    cx: &mut AsyncApp,
) -> Result<T, String> {
    let task = page.read_with(cx, |page, cx| page.eval::<T>(js, EVAL, cx));
    task.await
        .map_err(|error| format!("the page did not answer: {error}"))
}

async fn read(
    browser: &Entity<Browser>,
    page: &Entity<WebView>,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    let id = browser.read_with(cx, |browser, _| browser.id);
    let page: Page = eval(page, &script(SNAPSHOT), cx).await?;
    Ok(described(id, &page))
}

fn described(id: u64, page: &Page) -> String {
    let mut out = format!("tab {id}: {}\n{}\n\n{}", page.title, page.url, page.text);
    if page.truncated {
        out.push_str("\n\n[text truncated; scroll to reach the rest]");
    }
    if !page.elements.is_empty() {
        out.push_str("\n\nelements:");
        for element in &page.elements {
            let _ = write!(out, "\n[{}] {}", element.i, element.tag);
            if let Some(kind) = &element.kind {
                let _ = write!(out, " ({kind})");
            }
            if !element.name.is_empty() {
                let _ = write!(out, " {}", element.name);
            }
        }
    }
    out
}

fn tabs(panel: &Panel, cx: &Context<Panel>) -> String {
    let browsers = panel.browsers();
    if browsers.is_empty() {
        return "no browser tabs open in this project".to_owned();
    }
    browsers
        .iter()
        .map(|(browser, front)| {
            let browser = browser.read(cx);
            format!(
                "- tab {}: {} — {}{}",
                browser.id,
                browser.title(),
                browser.url(),
                if *front { " (front)" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
