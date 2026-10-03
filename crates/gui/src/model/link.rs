//! What a bookmark says about its URL: the page's Open Graph data, fetched on
//! first paint and held for the rest of the run.
//!
//! Installed as `markdown`'s link preview. A miss starts one fetch on its own
//! thread and paints the bare card; the answer lands in the cache and a
//! repaint is asked for. A failed fetch is cached as an empty preview and not
//! retried until the next launch.

use markdown::AppExt as _;
use std::{
    collections::HashMap,
    io::Read,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use bezel::gpui::{App, SharedString};
use futures::{StreamExt, channel::mpsc};
use markdown::Preview;
use url::Url;

/// The most of a page read looking for its `<head>`.
const LIMIT: u64 = 512 * 1024;

enum Entry {
    Pending,
    Done(Preview),
}

static CACHE: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
static LANDED: OnceLock<mpsc::UnboundedSender<()>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Entry>> {
    CACHE.get_or_init(Default::default)
}

/// Installs the preview and the task that repaints when a fetch lands. Once,
/// at boot.
pub fn init(cx: &mut App) {
    let (send, mut landed) = mpsc::unbounded();
    if LANDED.set(send).is_err() {
        return;
    }
    cx.set_link_preview(preview);
    cx.spawn(async move |cx| {
        while landed.next().await.is_some() {
            cx.update(|cx| cx.refresh_windows());
        }
    })
    .detach();
}

fn preview(url: &str, cx: &App) -> Option<Preview> {
    if url.starts_with(crate::view::entry_link::SCHEME) {
        return crate::view::mention::preview(url, cx);
    }
    let mut entries = cache().lock().ok()?;
    match entries.get(url) {
        Some(Entry::Done(preview)) => return Some(preview.clone()),
        Some(Entry::Pending) => return None,
        None => {}
    }
    let Ok(parsed) = Url::parse(url) else {
        entries.insert(url.to_string(), Entry::Done(Preview::default()));
        return None;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        entries.insert(url.to_string(), Entry::Done(Preview::default()));
        return None;
    }
    entries.insert(url.to_string(), Entry::Pending);
    drop(entries);
    let key = url.to_string();
    std::thread::spawn(move || {
        let preview = fetch(&parsed).unwrap_or_else(|| Preview {
            label: label(&parsed),
            ..Preview::default()
        });
        if let Ok(mut cache) = cache().lock() {
            cache.insert(key, Entry::Done(preview));
        }
        if let Some(landed) = LANDED.get() {
            let _ = landed.unbounded_send(());
        }
    });
    None
}

fn fetch(url: &Url) -> Option<Preview> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent("Mozilla/5.0 (compatible; Cydonia link preview)")
        .build()
        .new_agent();
    if let Some(id) = post(url) {
        return x(&agent, url, id);
    }
    let mut response = agent
        .get(url.as_str())
        .header("Accept", "text/html,application/xhtml+xml")
        .call()
        .ok()?;
    let html = response.headers().get("content-type").is_none_or(|kind| {
        kind.to_str()
            .is_ok_and(|kind| kind.contains("html") || kind.contains("xml"))
    });
    if !html {
        return Some(Preview {
            label: label(url),
            ..Preview::default()
        });
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(parse(&String::from_utf8_lossy(&bytes), url))
}

/// The preview a page's `<head>` describes, with relative URLs resolved
/// against `base`.
pub fn parse(html: &str, base: &Url) -> Preview {
    let head = match find(html, "</head") {
        Some(end) => &html[..end],
        None => html,
    };
    let mut meta: HashMap<String, String> = HashMap::new();
    let mut icon = None;
    let mut title = None;
    let mut rest = head;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        let Some(close) = rest.find('>') else { break };
        let attrs = attributes(&rest[name_end..close]);
        rest = &rest[close + 1..];
        match name.as_str() {
            "meta" => {
                let key = attrs
                    .get("property")
                    .or_else(|| attrs.get("name"))
                    .map(|key| key.to_ascii_lowercase());
                if let (Some(key), Some(content)) = (key, attrs.get("content")) {
                    meta.entry(key).or_insert_with(|| content.clone());
                }
            }
            "link" => {
                let rel = attrs.get("rel").map(|rel| rel.to_ascii_lowercase());
                let is_icon = rel
                    .as_deref()
                    .is_some_and(|rel| rel.split_whitespace().any(|word| word == "icon"));
                if is_icon && icon.is_none() {
                    icon = attrs.get("href").cloned();
                }
            }
            "title" if title.is_none() => {
                if let Some(end) = find(rest, "</title") {
                    title = Some(decode(rest[..end].trim()));
                }
            }
            _ => {}
        }
    }
    let pick = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| meta.get(*key))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let text = |value: String| Some(SharedString::from(value));
    let link = |value: String| {
        base.join(&value)
            .ok()
            .map(|url| SharedString::from(url.to_string()))
    };
    Preview {
        title: pick(&["og:title", "twitter:title"])
            .or(title.filter(|title| !title.is_empty()))
            .and_then(text),
        description: pick(&["og:description", "twitter:description", "description"]).and_then(text),
        image: pick(&[
            "og:image",
            "og:image:url",
            "twitter:image",
            "twitter:image:src",
        ])
        .and_then(link),
        icon: icon
            .or_else(|| Some("/favicon.ico".to_string()))
            .and_then(link),
        label: label(base),
        glyph: None,
    }
}

/// The id of the post `url` names: `x.com/<user>/status/<id>`, on any of X's
/// hosts.
fn post(url: &Url) -> Option<&str> {
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .trim_start_matches("mobile.");
    if !matches!(host, "x.com" | "twitter.com") {
        return None;
    }
    let mut segments = url.path_segments()?;
    segments.next().filter(|user| !user.is_empty())?;
    segments.next().filter(|status| *status == "status")?;
    segments
        .next()
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// An X post: its pages carry no Open Graph data for a client that is not a
/// known crawler. The words come from oEmbed, X's documented endpoint; the
/// picture from the syndication endpoint behind X's embeds, which is
/// undocumented and also answers for posts oEmbed refuses.
fn x(agent: &ureq::Agent, url: &Url, id: &str) -> Option<Preview> {
    let oembed = get_json(agent.get("https://publish.x.com/oembed").query_pairs([
        ("url", url.as_str()),
        ("omit_script", "1"),
        ("dnt", "true"),
    ]))
    .map(|body| tweet(&body));
    let syndicated = get_json(
        agent
            .get("https://cdn.syndication.twimg.com/tweet-result")
            .query_pairs([("id", id), ("token", &token(id))]),
    )
    .map(|body| syndication(&body));
    match (oembed, syndicated) {
        (Some(words), Some(picture)) => Some(Preview {
            image: picture.image,
            ..words
        }),
        (words, picture) => words.or(picture),
    }
}

fn get_json(
    request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
) -> Option<serde_json::Value> {
    let body = request.call().ok()?.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

/// The token the syndication endpoint is asked with: `(id / 1e15) * π` in
/// base 36, without its zeros or its point.
fn token(id: &str) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let value = id.parse::<f64>().unwrap_or(0.0) / 1e15 * std::f64::consts::PI;
    let (mut whole, mut fraction) = (value.trunc() as u64, value.fract());
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(whole % 36) as usize]);
        whole /= 36;
        if whole == 0 {
            break;
        }
    }
    out.reverse();
    for _ in 0..11 {
        fraction *= 36.0;
        out.push(DIGITS[fraction.trunc() as usize]);
        fraction = fraction.fract();
    }
    out.into_iter()
        .filter(|digit| *digit != b'0')
        .map(char::from)
        .collect()
}

/// The preview an X oEmbed response describes.
pub fn tweet(body: &serde_json::Value) -> Preview {
    let field = |key: &str| body.get(key).and_then(|value| value.as_str());
    let handle = field("author_url")
        .and_then(|author| author.trim_end_matches('/').rsplit('/').next())
        .filter(|handle| !handle.is_empty());
    let text = field("html").and_then(|html| {
        let start = find(html, "<p")?;
        let open = start + html[start..].find('>')? + 1;
        let end = open + find(&html[open..], "</p")?;
        Some(decode(&strip(&html[open..end])))
    });
    post_preview(field("author_name"), handle, text.as_deref(), None)
}

/// The preview an X syndication response describes.
pub fn syndication(body: &serde_json::Value) -> Preview {
    let text = |value: &serde_json::Value| value.as_str().map(str::to_string);
    let user = &body["user"];
    let image = body["photos"][0]["url"]
        .as_str()
        .or_else(|| body["mediaDetails"][0]["media_url_https"].as_str())
        .or_else(|| body["video"]["poster"].as_str())
        .map(str::to_string)
        .or_else(|| {
            let avatar = user["profile_image_url_https"].as_str()?;
            Some(avatar.replace("_normal.", "_400x400."))
        });
    post_preview(
        user["name"].as_str(),
        user["screen_name"].as_str(),
        text(&body["text"]).as_deref(),
        image,
    )
}

/// A post's card: its author as the title, its words without the media links
/// X appends, and the handle in the footer.
fn post_preview(
    name: Option<&str>,
    handle: Option<&str>,
    text: Option<&str>,
    image: Option<String>,
) -> Preview {
    let handle = handle.map(|handle| format!("@{handle}"));
    let title = match (name, &handle) {
        (Some(name), Some(handle)) => Some(format!("{name} ({handle})")),
        (Some(name), None) => Some(name.to_string()),
        (None, handle) => handle.clone(),
    };
    let text = text.map(|text| {
        let mut words: Vec<&str> = text.split_whitespace().collect();
        while words.last().is_some_and(|word| {
            ["pic.twitter.com/", "pic.x.com/", "https://t.co/"]
                .iter()
                .any(|link| word.starts_with(link))
        }) {
            words.pop();
        }
        words.join(" ")
    });
    Preview {
        title: title.map(Into::into),
        description: text.filter(|text| !text.is_empty()).map(Into::into),
        image: image.map(Into::into),
        icon: Some("https://abs.twimg.com/favicons/twitter.3.ico".into()),
        label: handle.map(Into::into),
        glyph: None,
    }
}

/// `html` with its tags removed; a `<br>` reads as a space.
fn strip(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let Some(close) = rest.find('>') else { break };
        out.push(' ');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The unit a URL names where its host is not the most specific one.
fn label(url: &Url) -> Option<SharedString> {
    let host = url.host_str()?.trim_start_matches("www.");
    let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
    match host {
        "github.com" | "gitlab.com" | "codeberg.org" => {
            let owner = segments.next()?;
            let repo = segments.next()?;
            Some(format!("{host}/{owner}/{repo}").into())
        }
        "reddit.com" | "old.reddit.com" => {
            (segments.next()? == "r").then_some(())?;
            Some(format!("r/{}", segments.next()?).into())
        }
        _ => None,
    }
}

/// The byte offset of `needle` in `haystack`, ignoring ASCII case.
fn find(haystack: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// A tag's attributes, names lowercased and values decoded.
fn attributes(source: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = source.trim_start_matches('/');
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            break;
        }
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            out.entry(name).or_default();
            continue;
        };
        rest = after.trim_start();
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let body = &rest[1..];
                let end = body.find(quote).unwrap_or(body.len());
                rest = body.get(end + 1..).unwrap_or("");
                &body[..end]
            }
            _ => {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let value = &rest[..end];
                rest = &rest[end..];
                value
            }
        };
        out.entry(name).or_insert_with(|| decode(value));
    }
    out
}

/// `text` with its character references replaced.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.bytes().take(12).position(|b| b == b';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let named = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "hellip" => Some('\u{2026}'),
            _ => None,
        };
        let numeric = || {
            let code = match entity.strip_prefix('#')? {
                hex if hex.starts_with(['x', 'X']) => u32::from_str_radix(&hex[1..], 16).ok()?,
                dec => dec.parse().ok()?,
            };
            char::from_u32(code)
        };
        match named.or_else(numeric) {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
