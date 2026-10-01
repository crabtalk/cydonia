//! Pictures a document points at: where a pasted screenshot's bytes go, and
//! how the ones a message carries reach an agent.
//!
//! A document holds a URL, and bytes off the clipboard have no address
//! anywhere — so somewhere has to be picked before an image block can exist,
//! which is what `editor::AppExt::set_image_store` asks the app. The answer is the
//! open article's own `assets/`, the same directory agents are told to write
//! into, so what the app pastes and what an agent generates land together and
//! a picture is named for its bytes wherever it came from. A session's
//! attachments go to the project's shared `assets/` instead — a transcript is
//! not an article.
//!
//! The editor names which document is asking, but what it hands over is an
//! `Entity<Editor>` and the article behind one is the workspace's to know —
//! so which directory to write into is still noted by [`aim`] as a document
//! opens. One window and one document in front of it, so one target.

use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(feature = "desktop")]
use bezel::gpui::WeakEntity;
use bezel::gpui::{App, ClipboardEntry, ClipboardItem, Entity, Image, hash};
use editor::AppExt as _;
use editor::{Editor, ImageStore, Mode, PasteContent, PasteContext, Source};
use image::{ImageFormat, imageops::FilterType};
use markdown::BlockKind;
use std::{
    borrow::Cow,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

/// What a picture's file name begins with.
const MARK: &str = "media-";

/// The longest edge a picture is sent to an agent at. Larger ones are scaled
/// down on the way out, since a model reads no more than this and the base64
/// of a retina screenshot is megabytes of prompt.
pub const LONG_EDGE: u32 = 1568;

/// A picture picked in the composer, held until the message is sent — the
/// composer does not know which project the session it feeds is in.
#[derive(Clone)]
pub enum Attachment {
    Bytes(Arc<Image>),
    File(PathBuf),
}

/// Install the store. Called once, beside the other `init`s.
///
/// `accepts` is left at the editor's own guess from the extension — cydonia
/// decodes nothing the default would turn away.
///
/// `pasting` is what `settings.toml` says about pasted pictures.
pub fn init(pasting: Pasting, cx: &mut App) {
    cx.set_image_store(ImageStore {
        keep,
        ..ImageStore::default()
    });
    cx.set_paste_handler(paste);
    cx.set_global(pasting);
}

/// What becomes of a pasted picture.
#[derive(Clone, Copy, Default)]
pub struct Pasting {
    /// Whether a picture's web address is downloaded — see [`fetch_into`].
    pub fetch: bool,
    /// Whether source mode takes a picture — bytes, a file or a web address
    /// — as an image line. Off, source mode pastes as the editor does.
    pub source: bool,
}

impl bezel::gpui::Global for Pasting {}

/// Change what becomes of a pasted picture from now on.
pub fn set_pasting(pasting: Pasting, cx: &mut App) {
    cx.set_global(pasting);
}

/// A picture's web address goes in as a picture, and is fetched into the
/// document's `assets/` when [`Pasting::fetch`] is on — see [`fetch_into`]. A
/// picture's bytes or file pasted into source mode is kept the way a rich
/// paste keeps it and written in as an image line at the caret. Source mode
/// does either only when [`Pasting::source`] is on. Anything else is left to
/// the editor.
fn paste(
    item: &ClipboardItem,
    editor: &Entity<Editor>,
    at: PasteContext<'_>,
    cx: &mut App,
) -> Option<PasteContent> {
    let pasting = cx.try_global::<Pasting>().copied().unwrap_or_default();
    if at.mode == Mode::Source && !pasting.source {
        return None;
    }
    if let Some(url) = item.text().map(|text| text.trim().to_owned())
        && markdown::is_url(&url)
        && markdown::is_image(&url)
        && (at.mode == Mode::Source || !at.in_fence)
    {
        #[cfg(feature = "desktop")]
        if pasting.fetch
            && let Some(base) = at.base
        {
            fetch_into(url.clone(), base.to_path_buf(), editor.downgrade(), cx);
        }
        let line = format!("![]({url})");
        return Some(match at.mode {
            Mode::Source => PasteContent::Literal(line),
            Mode::Blocks => PasteContent::Markdown(line),
        });
    }
    if at.mode != Mode::Source {
        return None;
    }
    let store = cx.image_store();
    let urls: Vec<String> = item.entries().iter().find_map(|entry| {
        let urls: Vec<String> = match entry {
            ClipboardEntry::Image(image) => (store.keep)(Source::Bytes(image), editor, at.base, cx)
                .into_iter()
                .collect(),
            ClipboardEntry::ExternalPaths(paths) => paths
                .paths()
                .iter()
                .filter(|path| (store.accepts)(path))
                .filter_map(|path| (store.keep)(Source::File(path), editor, at.base, cx))
                .collect(),
            ClipboardEntry::String(_) => Vec::new(),
        };
        (!urls.is_empty()).then_some(urls)
    })?;
    let lines: Vec<String> = urls.iter().map(|url| format!("![]({url})")).collect();
    Some(PasteContent::Literal(lines.join("\n")))
}

/// The most a picture fetched off the web may weigh.
#[cfg(feature = "desktop")]
const FETCH_LIMIT: u64 = 25 * 1024 * 1024;

/// Fetch the picture at `url` into the assets beside `base`, then point every
/// picture in `editor` still at `url` at the copy. A fetch that fails
/// leaves the web address where it is.
#[cfg(feature = "desktop")]
fn fetch_into(url: String, base: PathBuf, editor: WeakEntity<Editor>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let fetched = cx
            .background_executor()
            .spawn({
                let url = url.clone();
                async move {
                    let path = url.split(['?', '#']).next().unwrap_or(&url);
                    let extension = extension(Path::new(path))?;
                    let agent = ureq::Agent::config_builder()
                        .timeout_global(Some(std::time::Duration::from_secs(30)))
                        .build()
                        .new_agent();
                    let bytes = agent
                        .get(&url)
                        .call()
                        .ok()?
                        .body_mut()
                        .with_config()
                        .limit(FETCH_LIMIT)
                        .read_to_vec()
                        .ok()?;
                    image::guess_format(&bytes).ok()?;
                    let dir = artifact::article::assets(&artifact::article::content(&base));
                    let file = store(&dir, &bytes, &extension)?;
                    let file = file.strip_prefix(&base).unwrap_or(&file).to_owned();
                    Some(file.to_string_lossy().into_owned())
                }
            })
            .await;
        let Some(local) = fetched else {
            return;
        };
        let _ = editor.update(cx, |editor, cx| editor.relink(&url, &local, cx));
    })
    .detach();
}

/// Write `bytes` into `dir` under a name taken from their hash, so the same
/// picture kept twice is the one file — and gpui, which caches a decoded
/// picture against its path, is handed the copy it already has.
pub fn store(dir: &Path, bytes: &[u8], extension: &str) -> Option<PathBuf> {
    let file = dir.join(format!("{MARK}{:x}.{extension}", hash(&bytes)));
    if !file.is_file() {
        std::fs::create_dir_all(dir).ok()?;
        std::fs::write(&file, bytes).ok()?;
    }
    Some(file)
}

/// Keep an attachment in `dir`. A file is copied in rather than pointed at
/// where it is: a transcript that outlives the download it was sent from is
/// the reason the picture lives with the project.
pub fn keep_attachment(dir: &Path, attachment: &Attachment) -> Option<PathBuf> {
    match attachment {
        Attachment::Bytes(image) => store(dir, &image.bytes, image.format.extension()),
        Attachment::File(path) => store(dir, &std::fs::read(path).ok()?, &extension(path)?),
    }
}

/// A picture as a message line. The destination is bracketed, since a
/// project path is free to have a space in it.
pub fn line(path: &Path) -> String {
    format!("![](<{}>)", path.display())
}

/// The local pictures a message points at, in order.
pub fn attached(text: &str) -> Vec<PathBuf> {
    markdown::parse(text)
        .blocks
        .into_iter()
        .filter_map(|block| match block.kind {
            BlockKind::Image { url, .. } if !url.is_empty() && !url.contains("://") => {
                Some(PathBuf::from(url))
            }
            _ => None,
        })
        .collect()
}

/// Restore local image blocks as composer attachments.
pub fn detach(text: &str) -> (String, Vec<PathBuf>) {
    let mut doc = markdown::parse(text);
    let mut attachments = Vec::new();
    doc.blocks.retain(|block| {
        if let BlockKind::Image { url, .. } = &block.kind
            && !url.is_empty()
            && !url.contains("://")
        {
            attachments.push(PathBuf::from(url));
            return false;
        }
        true
    });
    let text = if attachments.is_empty() {
        text.to_owned()
    } else {
        markdown::serialize(&doc)
    };
    (text, attachments)
}

/// A picture as an agent is handed one: base64 and its MIME type. Sent as it
/// is when it is already small and in a format models read, and otherwise
/// scaled to [`LONG_EDGE`] and written as a PNG.
pub fn encode(path: &Path) -> Option<(String, &'static str)> {
    let bytes = std::fs::read(path).ok()?;
    let format = image::guess_format(&bytes).ok()?;
    let picture = image::load_from_memory_with_format(&bytes, format).ok()?;
    let fits = picture.width().max(picture.height()) <= LONG_EDGE;
    let mime = match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::Gif => Some("image/gif"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    };
    if let (true, Some(mime)) = (fits, mime) {
        return Some((STANDARD.encode(&bytes), mime));
    }
    let picture = match fits {
        true => picture,
        false => picture.resize(LONG_EDGE, LONG_EDGE, FilterType::Lanczos3),
    };
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, ImageFormat::Png).ok()?;
    Some((STANDARD.encode(out.into_inner()), "image/png"))
}

fn extension(path: &Path) -> Option<String> {
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

/// Take a picture into the open project's assets, and answer with what the
/// document is to point at.
///
/// An absolute path rather than a relative one: what paints the picture reads
/// the URL as a path off this process, whose working directory is not the
/// project's.
/// Into the `assets/` beside the editor's base, answered relative to it: an
/// article's own folder, or for a card its project's `.cydonia`, so a card's
/// pictures land in the project's shared `assets/`. An editor with no base
/// lets the picture go.
fn keep(source: Source, _: &Entity<Editor>, base: Option<&Path>, _: &App) -> Option<String> {
    let base = base?.to_path_buf();
    let dir = artifact::article::assets(&artifact::article::content(&base));
    let (bytes, extension) = match source {
        Source::Bytes(image) => (
            Cow::Borrowed(image.bytes.as_slice()),
            image.format.extension().to_owned(),
        ),
        Source::File(path) => (Cow::Owned(std::fs::read(path).ok()?), extension(path)?),
    };
    let file = store(&dir, &bytes, &extension)?;
    let file = file.strip_prefix(&base).unwrap_or(&file);
    Some(file.to_string_lossy().into_owned())
}
