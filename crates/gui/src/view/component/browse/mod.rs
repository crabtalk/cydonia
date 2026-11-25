//! Browser commands for a right-panel tab or a canvas web frame.
//!
//! Elements are addressed by the number a read gave them. Every script derives
//! the list with [`FIND`], so a number from a read names the same element when
//! the next call derives the list again.
//!
//! Scripts run through [`WebView::eval`], which exists only once a render has
//! built the page: a tab whose panel is not on screen has none.

use crate::{
    model::{media::Png as _, settings::Browsing},
    view::{
        component::{browser::Browser, panel::Panel},
        design::preview::{self, LiveArtboard},
        pane::Tab,
        window::root::Cydonia,
    },
};
use artifact::{kind::Kind, project::Project as _};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bezel::gpui::{AnyWindowHandle, App, AsyncApp, Context, Entity, WeakEntity, Window};
use browser::WebView;
use futures::{StreamExt as _, channel::mpsc};
use mcp::{
    rail::{self, Act, Browse, PageTarget, Target},
    tool::{Answer, Image},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::{fmt::Write as _, path::Path, time::Duration};

pub(crate) mod console;

/// Characters of a page's text a read carries. Sliced in the page, so the rest
/// never crosses the bridge.
const MAX_TEXT: usize = 8000;

/// Elements listed per read.
const MAX_ELEMENTS: usize = 200;

/// How long one script may run.
const EVAL: Duration = Duration::from_secs(10);

/// How long a tab is given to build its page after it is opened.
const BUILT: Duration = Duration::from_secs(12);

/// How long a page is given to finish loading after an action.
const SETTLE: Duration = Duration::from_secs(10);

const STEP: Duration = Duration::from_millis(100);

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

const INSPECT: &str = include_str!("inspect.js");

const EVALUATE: &str = r#"(async () => {
  const value = await (
$SCRIPT
);
  const json = JSON.stringify(value ?? null);
  if (json === undefined) throw new Error('result has no JSON representation');
  if (json.length > 16000) throw new Error('result exceeds 16000 characters; return a smaller selection');
  return { url: location.href, result: JSON.parse(json) };
})()"#;

#[derive(Deserialize)]
struct Evaluation {
    url: String,
    result: Value,
}

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

fn script(js: &str) -> String {
    js.replace("$FIND", FIND)
        .replace("$MAX_ELEMENTS", &MAX_ELEMENTS.to_string())
        .replace("$MAX_TEXT", &MAX_TEXT.to_string())
}

impl Cydonia {
    /// Answer the browser tools, one call at a time. Called once, with the
    /// window.
    pub(crate) fn take_browser(&self, window: &Window, cx: &mut Context<Self>) {
        let window = window.window_handle();
        let (asked, mut asks) = mpsc::unbounded();
        rail::install_browser(move |browse| {
            let _ = asked.unbounded_send(browse);
        });
        cx.spawn(async move |this, cx| {
            while let Some(Browse {
                project,
                target,
                act,
                reply,
            }) = asks.next().await
            {
                let answer = serve(&this, window, &project, target, act, cx).await;
                let _ = reply.send(answer);
                if this.upgrade().is_none() {
                    return;
                }
            }
        })
        .detach();
    }

    fn shown_frame(
        &mut self,
        project: &Path,
        canvas: &str,
        node: &str,
        cx: &mut Context<Self>,
    ) -> Result<Entity<crate::view::canvas::web::Page>, String> {
        let project = self
            .workspace
            .update(cx, |workspace, _| workspace.intern(project));
        for leaf in self.shown_tabs(cx) {
            if let Some(view) = leaf.read(cx).canvas_shown(cx) {
                let view = view.read(cx);
                if view.canvas.id == canvas && view.canvas.project == project {
                    return view.web_frame(node, cx);
                }
            }
        }
        Err("the canvas is not showing; open it to use its web frame".into())
    }

    fn shown_tabs(&self, cx: &App) -> Vec<Entity<Tab>> {
        let Some(space) = self.arrangement(cx).filter(|_| !self.library_shown(cx)) else {
            return Vec::new();
        };
        let fronts = space
            .zoomed()
            .map_or_else(|| space.tree.fronts(), |entry| vec![entry]);
        self.leaves
            .get(self.focused)
            .into_iter()
            .chain(
                self.leaves
                    .iter()
                    .enumerate()
                    .filter_map(|(ix, leaf)| (ix != self.focused).then_some(leaf)),
            )
            .filter(|leaf| fronts.contains(&leaf.read(cx).entry))
            .cloned()
            .collect()
    }

    fn design_artboard(
        &mut self,
        project: &Path,
        id: &str,
        artboard: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(LiveArtboard, bool), String> {
        let project = self
            .workspace
            .update(cx, |workspace, _| workspace.intern(project));
        let store = self
            .workspace
            .read(cx)
            .project(project)
            .ok_or("design project is closed")?
            .store();
        let design = store.design(id).ok_or("design is no longer available")?;
        let artboard = design
            .artboards
            .iter()
            .find(|held| held.id == artboard)
            .ok_or_else(|| format!("no artboard {artboard} in this design"))?;
        for leaf in self.shown_tabs(cx) {
            if let Some(view) = leaf.read(cx).design_shown(cx) {
                let view = view.read(cx);
                if view.entry.id == id
                    && view.entry.project == project
                    && let Some(live) = view.live_artboard(&artboard.id, design.viewport, cx)
                {
                    return Ok((live, false));
                }
            }
        }
        let folder = store
            .place(Kind::Design, id)
            .ok_or("design directory is unavailable")?;
        let live = preview::open(&folder, artboard, design.viewport, false, window, cx)?;
        self.browser_tool_page = Some(live.browser.clone());
        cx.notify();
        Ok((live, true))
    }

    /// The right panel of the open project at `path`.
    fn project_panel(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Panel>> {
        let id = self
            .workspace
            .update(cx, |workspace, _| workspace.intern(path));
        let project = self.workspace.read(cx).project(id)?.path.clone();
        let cwd = if self
            .workspace
            .read(cx)
            .active_project()
            .is_some_and(|active| active.path == project)
        {
            self.shell_cwd(cx).unwrap_or(project)
        } else {
            project
        };
        Some(self.right_panel(cwd, window, cx))
    }
}

async fn serve(
    this: &WeakEntity<Cydonia>,
    window: AnyWindowHandle,
    project: &Path,
    target: PageTarget,
    act: Act,
    cx: &mut AsyncApp,
) -> Result<Answer, String> {
    let tab = match target {
        PageTarget::Tab(tab) => tab,
        PageTarget::Frame {
            canvas,
            number,
            node,
        } => {
            let frame = this
                .update(cx, |this, cx| this.shown_frame(project, &canvas, &node, cx))
                .map_err(|error| error.to_string())??;
            let page = frame.read_with(cx, |frame, _| frame.webview())?;
            if !frame.read_with(cx, |frame, cx| frame.ready(cx)) {
                return Err(
                    "web frame has no live page; bring it into view and wait for it to load".into(),
                );
            }
            let address = format!("canvas #{number}, node {node}");
            return match act {
                Act::Console { levels, since } => {
                    check_page(&page, cx).await?;
                    Ok(Answer::said(frame.read_with(cx, |frame, _| {
                        frame.console.read(&levels, since)
                    })))
                }
                act => on_page(&address, &page, act, cx).await,
            };
        }
        PageTarget::Design {
            design,
            number,
            artboard,
        } => {
            let (live, temporary) = window
                .update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.design_artboard(project, &design, &artboard, window, cx)
                    })
                })
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())??;
            let address = format!("design #{number}, artboard {artboard}");
            let answer = match live.ready(cx).await {
                Err(error) => Err(error),
                Ok(()) => match act {
                    Act::Console { levels, since } => check_page(&live.browser, cx)
                        .await
                        .map(|_| Answer::said(live.console.borrow().read(&levels, since))),
                    act => on_page(&address, &live.browser, act, cx).await,
                },
            };
            if temporary {
                let _ = this.update(cx, |this, cx| {
                    this.browser_tool_page = None;
                    cx.notify();
                });
            }
            return answer;
        }
    };
    let panel = window
        .update(cx, |_, window, cx| {
            this.update(cx, |this, cx| this.project_panel(project, window, cx))
        })
        .ok()
        .and_then(Result::ok)
        .flatten()
        .ok_or_else(|| format!("cydonia does not have {} open", project.display()))?;
    let browsing = cx.update(|cx| cx.try_global::<Browsing>().cloned().unwrap_or_default());
    if let Act::Open(url) = &act {
        refuse_blocked(&browsing, url)?;
        window
            .update(cx, |_, window, cx| {
                panel.update(cx, |panel, cx| panel.restore_tabs(window, cx));
            })
            .map_err(|error| error.to_string())?;
    }
    // The tab's address as the call arrives: a page an agent is refused is
    // judged by where it is, not by where the call says it is going.
    if !matches!(act, Act::Tabs) && (tab.is_some() || !matches!(act, Act::Open(_))) {
        let browser = pick(&panel, tab, cx)?;
        let url = browser.read_with(cx, |browser, _| browser.url().to_owned());
        refuse_blocked(&browsing, &url)?;
    }
    match act {
        Act::Tabs => Ok(panel.read_with(cx, |panel, cx| Answer::said(tabs(panel, cx)))),
        Act::Open(url) => {
            refuse_blocked(&browsing, &url)?;
            let browser = match tab {
                Some(_) => {
                    let browser = pick(&panel, tab, cx)?;
                    browser.update(cx, |browser, cx| browser.navigate(url, cx));
                    browser
                }
                None => match showing(&panel, &url, cx) {
                    Some(browser) => browser,
                    None => panel
                        .update(cx, |panel, cx| panel.open_browser(url, cx))
                        .ok_or("browser tabs are switched off in cydonia's settings")?,
                },
            };
            panel.update(cx, |panel, cx| panel.activate_browser(&browser, cx));
            this.update(cx, |this, cx| this.reveal_browser_panel(&panel, cx))
                .map_err(|error| error.to_string())?;
            let page = built(&browser, cx).await?;
            settle(&page, cx).await;
            let address = browser.read_with(cx, |browser, _| format!("tab {}", browser.id));
            read(&address, &page, cx).await.map(Answer::said)
        }
        Act::Console { levels, since } => {
            let browser = pick(&panel, tab, cx)?;
            Ok(Answer::said(browser.read_with(cx, |browser, cx| {
                browser.console(&levels, since, cx)
            })))
        }
        act => {
            let browser = pick(&panel, tab, cx)?;
            let page = built(&browser, cx).await?;
            let address = browser.read_with(cx, |browser, _| format!("tab {}", browser.id));
            on_page(&address, &page, act, cx).await
        }
    }
}

async fn check_page(page: &Entity<WebView>, cx: &mut AsyncApp) -> Result<String, String> {
    let url: String = eval(page, "location.href", cx).await?;
    let browsing = cx.update(|cx| cx.try_global::<Browsing>().cloned().unwrap_or_default());
    refuse_blocked(&browsing, &url)?;
    Ok(url)
}

async fn on_page(
    address: &str,
    page: &Entity<WebView>,
    act: Act,
    cx: &mut AsyncApp,
) -> Result<Answer, String> {
    check_page(page, cx).await?;
    let text = match act {
        Act::Read => read(address, page, cx).await,
        Act::Screenshot => {
            settle(page, cx).await;
            check_page(page, cx).await?;
            let task = page.read_with(cx, |page, cx| page.snapshot(cx));
            let image = task.await?;
            let bytes = cx
                .background_executor()
                .spawn(async move { image.png().map_err(|error| error.to_string()) })
                .await?;
            let url = check_page(page, cx).await?;
            let mut answer = Answer::said(format!("{address}: {url}"));
            answer.images.push(Image {
                data: STANDARD.encode(bytes),
                mime_type: "image/png".to_owned(),
            });
            return Ok(answer);
        }
        Act::Evaluate(js) => evaluate(address, page, &js, cx).await,
        Act::Inspect(target) => {
            let target = match target {
                Target::Selector(selector) => format!(
                    "document.querySelector({})",
                    serde_json::to_string(&selector).unwrap()
                ),
                Target::Element(element) => format!("found[{element}]"),
            };
            let js = script(INSPECT).replace("$TARGET", &target);
            evaluate(address, page, &js, cx).await
        }
        Act::Click(element) => {
            let js = script(CLICK).replace("$N", &element.to_string());
            act_on(address, page, &js, element, cx).await
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
            act_on(address, page, &js, element, cx).await
        }
        Act::Scroll(pages) => {
            let js = SCROLL.replace("$PAGES", &format!("{:.2}", pages.clamp(-20., 20.)));
            act_on(address, page, &js, 0, cx).await
        }
        Act::Tabs | Act::Open(_) | Act::Console { .. } => {
            Err("command does not operate on this page".into())
        }
    };
    text.map(Answer::said)
}

fn refuse_blocked(browsing: &Browsing, url: &str) -> Result<(), String> {
    match browsing.blocks(url) {
        true => Err(format!(
            "{url} is on a site the user has kept agents off in cydonia's settings"
        )),
        false => Ok(()),
    }
}

/// Run `js` in the page and read it after navigation settles.
async fn act_on(
    address: &str,
    page: &Entity<WebView>,
    js: &str,
    element: usize,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    match eval::<String>(page, js, cx).await?.as_str() {
        "ok" => {}
        "gone" => {
            return Err(format!(
                "element {element} is not on the page any more; read the page for its current elements"
            ));
        }
        other => return Err(other.to_owned()),
    }
    // A click that navigates starts its load a moment after the script ends.
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    settle(page, cx).await;
    read(address, page, cx).await
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

/// The project's tab already at `url`, fragments aside, the front one first.
fn showing(panel: &Entity<Panel>, url: &str, cx: &mut AsyncApp) -> Option<Entity<Browser>> {
    let wanted = same_page(url);
    let mut browsers = panel.read_with(cx, |panel, _| panel.browsers());
    browsers.sort_by_key(|(_, front)| !*front);
    cx.update(|cx| {
        browsers
            .into_iter()
            .map(|(browser, _)| browser)
            .find(|browser| same_page(browser.read(cx).url()) == wanted)
    })
}

/// `url` as two addresses of one page compare: parsed, without its fragment.
fn same_page(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut url) => {
            url.set_fragment(None);
            url.into()
        }
        Err(_) => url.to_owned(),
    }
}

/// The tab's page, waiting a moment for a render to build it.
async fn built(browser: &Entity<Browser>, cx: &mut AsyncApp) -> Result<Entity<WebView>, String> {
    let mut waited = Duration::ZERO;
    loop {
        if let Some(page) = browser.read_with(cx, |browser, _| browser.webview()) {
            let ready = page.read_with(cx, |page, _| {
                if let Some(error) = &page.error {
                    Err(format!("browser unavailable: {error}"))
                } else {
                    Ok(page.can_evaluate())
                }
            })?;
            if ready {
                return Ok(page);
            }
        }
        if waited >= BUILT {
            return Err(
                "the browser page did not become ready; show its panel to start it, \
                or check the tab for a runtime error"
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
    let task = page.read_with(cx, |page, cx| {
        if let Some(error) = &page.error {
            return Err(format!("browser unavailable: {error}"));
        }
        Ok(page.eval::<T>(js, EVAL, cx))
    })?;
    task.await
        .map_err(|error| format!("the page did not answer: {error}"))
}

async fn evaluate(
    address: &str,
    page: &Entity<WebView>,
    js: &str,
    cx: &mut AsyncApp,
) -> Result<String, String> {
    let js = EVALUATE.replace("$SCRIPT", js);
    let task = page.read_with(cx, |page, cx| page.eval_async::<Evaluation>(&js, EVAL, cx));
    let result = task.await.map_err(|error| format!("JavaScript: {error}"))?;
    let browsing = cx.update(|cx| cx.try_global::<Browsing>().cloned().unwrap_or_default());
    refuse_blocked(&browsing, &result.url)?;
    Ok(format!(
        "{address}: {}\n\n{}",
        result.url,
        serde_json::to_string_pretty(&result.result).unwrap()
    ))
}

async fn read(address: &str, page: &Entity<WebView>, cx: &mut AsyncApp) -> Result<String, String> {
    let page: Page = eval(page, &script(SNAPSHOT), cx).await?;
    let browsing = cx.update(|cx| cx.try_global::<Browsing>().cloned().unwrap_or_default());
    refuse_blocked(&browsing, &page.url)?;
    Ok(described(address, &page))
}

fn described(address: &str, page: &Page) -> String {
    let mut out = format!("{address}: {}\n{}\n\n{}", page.title, page.url, page.text);
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

fn tabs(panel: &Panel, cx: &App) -> String {
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
