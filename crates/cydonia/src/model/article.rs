//! A project's articles: title, properties and content, under its `.cydonia/`.
//!
//! An article is a directory — `articles/untitled/` holding `content.md`, the
//! `properties.toml` beside it, and the cover. The markdown is content and
//! nothing else: an article is written to be read by the agent running in that
//! directory, and a path is how it gets handed over.
//!
//! The directory is named for when it was made, and nothing reads that name.
//! An id rather than a title: the title is a property, and a directory named
//! after it would be a second copy of it that a refused rename could leave
//! disagreeing — and a path already handed to an agent is not one we can
//! rewrite the way a vault rewrites its own links.

use crate::model::{
    cover, language,
    store::{self, Store},
    workspace::Workspace,
};
use artifact::{
    article as layout,
    article::properties::{self, Properties},
};
use bezel::{
    gpui::{App, AppContext as _, Context, Entity, ScrollHandle},
    ui::input::{Shape, TextField},
};
use editor::{Editor, Mode};
use markdown::Typography;
use std::path::{Path, PathBuf};
use url::Url;

/// What articles were called before they were named for their age, and what an
/// unnamed one was called among them.
const UNTITLED: &str = "untitled";

/// What a document with no title is shown as.
pub const UNNAMED: &str = "Untitled";

/// Claimed on the title field, so `enter` there moves to the body and stays a
/// newline in every other field.
pub const TITLE_CONTEXT: &str = "CydoniaArticleTitle";

pub struct Article {
    pub number: Option<u64>,
    /// What the project's backend names it by.
    pub id: String,
    /// Where its document is, derived from the project and the id whether or
    /// not the backend keeps files. What the app tells articles apart by.
    pub path: PathBuf,
    store: Store,
    /// The picture above the document, if it has been given one. See
    /// [`crate::model::cover`] — this is a cache of a file's existence, and the
    /// file is what decides.
    pub cover: Option<PathBuf>,
    /// What the document is called. Held here as well as in the field, because
    /// the sidebar labels articles nobody has opened.
    pub title: String,
    /// The title's field, once the article has been opened.
    pub field: Option<Entity<TextField>>,
    /// The editing surface, once the article has been opened. Building one for
    /// every article of every project at launch is the alternative.
    pub editor: Option<Entity<Editor>>,
    /// The pane's scroll box, shared with the editor so typing follows the
    /// caret down.
    pub scroll: ScrollHandle,
    /// Which form the document is edited in — see [`Article::set_mode`]. Held
    /// beside the editor rather than only in it, because a re-read builds a
    /// new editor and somebody reading the markdown did not ask to leave it.
    mode: Mode,
    /// What is on disk. The editor notifies on caret moves too, so without
    /// this every arrow key would rewrite the file.
    saved: String,
    /// When the document was last written. Held rather than read back per
    /// frame: the sidebar orders on it, and a project of a thousand articles
    /// would be a thousand `stat` calls a frame.
    pub touched: u128,
    /// Put away: listed under the divider rather than gone. Cached beside
    /// [`Article::touched`], and for the same reason.
    pub archived: bool,
    /// Set across the pane rather than in the reading column, and `None` for a
    /// page nobody has decided about — which follows the app's own default,
    /// see [`Article::wide`]. Cached like [`Article::archived`]: the frame
    /// reads it, and a frame is not somewhere to open a file.
    pub full_width: Option<bool>,
    /// The file moved under an open document that has edits of its own — see
    /// [`Article::adopt`]. Runtime only: what it marks is a disagreement
    /// between the buffer and the disk, and reopening the app ends it by
    /// reading the disk.
    pub stale: bool,
}

impl Article {
    fn new(project: &Path, store: Store, held: layout::Article) -> Self {
        let properties = store.properties(&held.id);
        Self {
            number: store.number("article", &held.id).ok(),
            path: layout::content(&layout::dir(project).join(&held.id)),
            cover: held.cover.and_then(|cover| cover.to_file_path().ok()),
            title: properties.title,
            touched: held.touched,
            archived: properties.archived,
            full_width: properties.full_width,
            id: held.id,
            store,
            field: None,
            editor: None,
            scroll: ScrollHandle::new(),
            mode: Mode::default(),
            saved: String::new(),
            stale: false,
        }
    }

    /// What is held beside the markdown, as the backend files it.
    fn properties(&self) -> Properties {
        Properties {
            title: self.title.clone(),
            archived: self.archived,
            full_width: self.full_width,
        }
    }

    pub fn unload(&mut self, cx: &App) {
        if !self.archived || self.edited(cx) {
            return;
        }
        self.field = None;
        self.editor = None;
        self.saved = String::new();
        self.scroll = ScrollHandle::new();
    }

    pub fn archive(&mut self, archived: bool) {
        self.archived = archived;
        let _ = self.store.save_properties(&self.id, &self.properties());
    }

    /// Set the page across the pane, or back in the column. `None` hands it
    /// back to the reader's default and takes the key out of the file.
    pub fn set_full_width(&mut self, wide: Option<bool>) {
        self.full_width = wide;
        let _ = self.store.save_properties(&self.id, &self.properties());
    }

    /// How wide this page is actually drawn, against the app's own default.
    /// One answer, so the pane and the menu that toggles it cannot disagree.
    pub fn wide(&self, default: bool) -> bool {
        self.full_width.unwrap_or(default)
    }

    /// Edit the markdown itself, or the document it spells. Runtime only: the
    /// form somebody is reading in is not a property of the file, and nothing
    /// on disk changes either way.
    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Workspace>) {
        self.mode = mode;
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| editor.set_mode(mode, cx));
        }
    }

    /// Which form the document is in. The editor's answer where there is one:
    /// an undo can step back over a switch, and the field here would not hear
    /// about it.
    pub fn mode(&self, cx: &App) -> Mode {
        match &self.editor {
            Some(editor) => editor.read(cx).mode(),
            None => self.mode,
        }
    }

    /// The sidebar's label.
    pub fn label(&self) -> &str {
        match self.title.is_empty() {
            true => UNNAMED,
            false => &self.title,
        }
    }

    /// Put a field over the title and an editor over the content. Idempotent —
    /// reopening an article is what keeps its undo history and its scroll.
    pub fn open(&mut self, text_size: f32, cx: &mut Context<Workspace>) {
        if self.editor.is_some() {
            return;
        }
        // The document's own heading type, so the title is set the way the page
        // would set its own first heading.
        let h1 = Typography::of(cx).h1;
        let title = self.title.clone();
        let field = cx.new(|cx| {
            let mut field = TextField::new(cx)
                .with_frame(false)
                // One line, because a title is: a pasted newline folds to a
                // space.
                .with_shape(Shape::Line)
                .with_key_context(TITLE_CONTEXT)
                .with_placeholder(UNNAMED)
                .with_metrics(h1);
            field.set_content(title, cx);
            field
        });
        cx.observe(&field, |workspace, field, cx| {
            workspace.write_article(field.entity_id(), cx);
        })
        .detach();

        self.saved = self.store.read_article(&self.id).unwrap_or_default();
        let scroll = self.scroll.clone();
        let editor = cx.new(|cx| {
            let editor = Editor::new(&self.saved, cx);
            let editor = match self.path.parent() {
                Some(dir) => editor.with_base(dir),
                None => editor,
            };
            editor
                .with_text_size(text_size)
                .with_scroll(scroll)
                .with_mode(self.mode)
        });
        language::ensure(fences(editor.read(cx)), cx);
        let mut source_digits = self.saved.split('\n').count().to_string().len();
        cx.observe(&editor, move |workspace, editor, cx| {
            workspace.write_article(editor.entity_id(), cx);
            // On every change rather than on open alone: a fence is usually
            // tagged after it is made, and the grammar is wanted the moment it
            // is named.
            language::ensure(fences(editor.read(cx)), cx);
            if editor.read(cx).mode() == Mode::Source {
                let digits = editor
                    .read(cx)
                    .source()
                    .split('\n')
                    .count()
                    .to_string()
                    .len();
                if digits != source_digits {
                    source_digits = digits;
                    cx.notify();
                }
            }
        })
        .detach();

        self.field = Some(field);
        self.editor = Some(editor);
    }

    /// Best effort, like every other write here: a document that cannot be
    /// saved is not worth failing a keystroke over.
    ///
    /// Each surface writes its own file, so typing in the body never touches
    /// the properties and naming the page never touches the markdown. Answers
    /// whether the title moved, which is what the caller repaints on.
    pub fn write(&mut self, cx: &App) -> bool {
        let renamed = match &self.field {
            Some(field) => {
                let title = field.read(cx).content().to_string();
                let moved = self.title != title;
                if moved {
                    self.title = title;
                    self.touched = artifact::stamp::now();
                    let _ = self.store.save_properties(&self.id, &self.properties());
                }
                moved
            }
            None => false,
        };
        if let Some(editor) = &self.editor {
            let source = editor.read(cx).source();
            if self.saved != source && self.store.write_article(&self.id, &source).is_ok() {
                self.saved = source;
                self.touched = artifact::stamp::now();
                // The buffer is the file again, whatever landed under it while
                // it was not — typing on is the third answer to the notice, and
                // it is the one most people will give.
                self.stale = false;
            }
        }
        renamed
    }

    /// Keep the buffer and write it over what landed on disk — the pane's other
    /// way out of the notice. The same write a keystroke makes, said out loud.
    pub fn keep(&mut self, cx: &App) {
        self.write(cx);
        self.stale = false;
    }

    /// Take what a re-read of the project found — see [`crate::model::watch`].
    ///
    /// The file wins, except where the document is open with edits that have
    /// not been written. There the buffer stands and the pane is told the file
    /// moved underneath it: an agent's write and a half-typed paragraph are
    /// both somebody's work, and this is not the layer that gets to choose.
    ///
    /// Answers whether the surfaces were replaced, which is what tells the pane
    /// the editor it had the caret in is not there any more.
    pub fn adopt(&mut self, fresh: &Self, cx: &mut Context<Workspace>) -> bool {
        self.number = fresh.number;
        self.cover = fresh.cover.clone();
        self.archived = fresh.archived;
        self.full_width = fresh.full_width;
        self.touched = fresh.touched;
        // Never opened: the label is the whole of what is held, and the file
        // is where it came from.
        if self.editor.is_none() {
            self.title = fresh.title.clone();
            return false;
        }
        // The echo of our own write, which every save produces. `saved` is what
        // this process last put on disk, so the two agreeing is the file saying
        // nothing new.
        let disk = self.store.read_article(&self.id).unwrap_or_default();
        if disk == self.saved && fresh.title == self.title {
            return false;
        }
        if self.edited(cx) {
            self.stale = true;
            return false;
        }
        self.revert(cx);
        true
    }

    /// Throw the surfaces away and build them again over what is on disk. What
    /// the pane's Reload does, and what [`Article::adopt`] does for a document
    /// with nothing of its own to lose.
    ///
    /// The undo history goes with the old editor. There is no honest way to
    /// keep it: it is a history of a document this one no longer is.
    pub fn revert(&mut self, cx: &mut Context<Workspace>) {
        // Carried over, since the surfaces are not: a file that moved under
        // the document is not somebody asking to leave the markdown.
        self.mode = self.mode(cx);
        let held = self.store.properties(&self.id);
        self.title = held.title;
        if let Some(article) = self.store.article(&self.id) {
            self.touched = article.touched;
            self.cover = article.cover.and_then(|cover| cover.to_file_path().ok());
        }
        self.archived = held.archived;
        self.full_width = held.full_width;
        let text_size = self
            .editor
            .as_ref()
            .and_then(|editor| editor.read(cx).text_size())
            .unwrap_or_else(bezel::theme::base_text_size);
        self.field = None;
        self.editor = None;
        self.open(text_size, cx);
        self.stale = false;
    }

    /// Whether either surface holds something the disk does not. The title is
    /// filed on the keystroke, so in practice this is the body — but a rename
    /// that failed to write leaves the field ahead of the file too.
    fn edited(&self, cx: &App) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).source() != self.saved)
            || self
                .field
                .as_ref()
                .is_some_and(|field| *field.read(cx).content() != self.title)
    }

    /// All of it: document, properties, cover and assets, and its number.
    pub fn remove(&self) {
        let _ = self.store.remove_article(&self.id);
    }

    /// Put a cover on the document, or take it off: `Some` brings that image
    /// in, `None` removes what is there.
    ///
    /// An import that fails leaves the cover that is already up. The person
    /// picked a file we could not read, and the answer to that is the picture
    /// they had, not a blank band.
    pub fn set_cover(&mut self, source: Option<&Path>) {
        let Some(source) = source else {
            self.replace_cover(None);
            return;
        };
        let seed = cover::seed(&self.path, self.cover.as_deref());
        let to = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .and_then(|ext| cover::path(&self.path, seed, &ext));
        if let Some(to) = to.filter(|to| cover::import(source, to).is_ok()) {
            self.replace_cover(Some(to));
        }
    }

    /// A fresh generated cover. Adding one and shuffling are the same act —
    /// the first cover an article is given is already a throw.
    pub fn shuffle_cover(&mut self) {
        let seed = cover::seed(&self.path, self.cover.as_deref());
        let Some(to) = cover::path(&self.path, seed, "svg") else {
            return;
        };
        if std::fs::write(&to, cover::svg(seed)).is_ok() {
            self.replace_cover(Some(to));
        }
    }

    /// Take down whatever is up and put this in its place. The old file goes
    /// with it, unless the new cover *is* the old file.
    fn replace_cover(&mut self, next: Option<PathBuf>) {
        let previous = std::mem::replace(&mut self.cover, next);
        if let Some(old) = previous.filter(|old| Some(old) != self.cover.as_ref()) {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// What a reader gets, out of what the pane holds.
///
/// The cover crosses as `file://` because this backend's covers *are* files.
/// Kept a conversion rather than a field: the picture is one this article
/// seeds from, writes and deletes, and every one of those wants the path back.
impl From<&Article> for layout::Article {
    fn from(article: &Article) -> Self {
        Self {
            id: article.id.clone(),
            title: article.title.clone(),
            archived: article.archived,
            touched: article.touched,
            cover: article
                .cover
                .as_deref()
                .and_then(|file| Url::from_file_path(file).ok()),
        }
    }
}

/// This project's articles, most recently touched first.
pub fn list(project: &Path) -> Vec<Article> {
    migrate(project);
    let store = store::open(project);
    store
        .articles()
        .into_iter()
        .map(|held| Article::new(project, store.clone(), held))
        .collect()
}

/// A new, empty document.
pub fn create(project: &Path) -> Option<Article> {
    let store = store::open(project);
    let held = store.create_article("").ok()?;
    Some(Article::new(project, store, held))
}

/// Articles used to sit loose in `.cydonia/` as `foo.md` beside `foo.cover-N.svg`,
/// and the stem was the name the sidebar showed. Give each one a directory of
/// its own age, and keep that stem by writing it in as the title it was.
///
/// Runs the first time a project is opened after the change; one with nothing
/// loose in it costs the `read_dir` [`list`] was about to do anyway.
fn migrate(project: &Path) {
    let Ok(entries) = std::fs::read_dir(layout::dir(project)) else {
        return;
    };
    let loose: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    for path in loose
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
    {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let to = layout::free(&layout::dir(project), artifact::stamp::of(path));
        if std::fs::create_dir_all(&to).is_err() {
            continue;
        }
        // The old name carried the document's stem so the two could sit in one
        // directory. Taking it off is what leaves today's `cover-<seed>.<ext>`.
        let worn = format!("{stem}.");
        for cover in loose.iter().filter_map(|cover| {
            cover
                .file_name()
                .and_then(|name| name.to_str())?
                .strip_prefix(&worn)
                .filter(|tail| tail.starts_with("cover-"))
                .map(|tail| (cover, tail))
        }) {
            let _ = std::fs::rename(cover.0, to.join(cover.1));
        }
        let content = layout::content(&to);
        if std::fs::rename(path, &content).is_ok() && !stem.starts_with(UNTITLED) {
            // Verbatim, slug and all: it is what the sidebar was already
            // showing, so nothing a person is looking at changes.
            properties::set_title(&content, stem);
        }
    }
}

/// The languages the document's fences are tagged with.
pub(crate) fn fences(editor: &Editor) -> Vec<String> {
    editor
        .doc()
        .blocks
        .iter()
        .filter_map(|block| match &block.kind {
            markdown::BlockKind::Code { language, .. } => language.clone(),
            _ => None,
        })
        .collect()
}
