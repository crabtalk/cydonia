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

use crate::model::{cover, workspace::Workspace};
use bezel::{
    gpui::{App, AppContext as _, Context, Entity, ScrollHandle},
    ui::input::{Shape, TextField},
};
use editor::Editor;
use markdown::Typography;
use schema::{article as layout, article::properties, project};
use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
};
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
    pub path: PathBuf,
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
    /// The file moved under an open document that has edits of its own — see
    /// [`Article::adopt`]. Runtime only: what it marks is a disagreement
    /// between the buffer and the disk, and reopening the app ends it by
    /// reading the disk.
    pub stale: bool,
}

impl Article {
    fn new(path: PathBuf) -> Self {
        Self {
            cover: cover::of(&path),
            title: properties::title(&path),
            touched: schema::stamp::of(&path),
            archived: properties::archived(&path),
            path,
            field: None,
            editor: None,
            scroll: ScrollHandle::new(),
            saved: String::new(),
            stale: false,
        }
    }

    pub fn archive(&mut self, archived: bool) {
        self.archived = archived;
        properties::set_archived(&self.path, archived);
    }

    /// Name it from outside the pane. The open title field is written too, or
    /// the next keystroke in the article would file the old name back.
    pub fn rename(&mut self, title: &str, cx: &mut App) {
        self.title = title.to_owned();
        self.touched = schema::stamp::now();
        properties::set_title(&self.path, title);
        if let Some(field) = &self.field {
            field.update(cx, |field, cx| field.set_content(title.to_owned(), cx));
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
    pub fn open(&mut self, cx: &mut Context<Workspace>) {
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

        self.saved = std::fs::read_to_string(&self.path).unwrap_or_default();
        let scroll = self.scroll.clone();
        let editor = cx.new(|cx| Editor::new(&self.saved, cx).with_scroll(scroll));
        cx.observe(&editor, |workspace, editor, cx| {
            workspace.write_article(editor.entity_id(), cx);
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
                    self.touched = schema::stamp::now();
                    properties::set_title(&self.path, &self.title);
                }
                moved
            }
            None => false,
        };
        if let Some(editor) = &self.editor {
            let source = editor.read(cx).source();
            if self.saved != source && std::fs::write(&self.path, &source).is_ok() {
                self.saved = source;
                self.touched = schema::stamp::now();
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
        self.cover = fresh.cover.clone();
        self.archived = fresh.archived;
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
        let disk = std::fs::read_to_string(&self.path).unwrap_or_default();
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
        self.title = properties::title(&self.path);
        self.touched = schema::stamp::of(&self.path);
        self.cover = cover::of(&self.path);
        self.archived = properties::archived(&self.path);
        self.field = None;
        self.editor = None;
        self.open(cx);
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

    /// All of it: the directory is the article.
    pub fn remove(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
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
            id: layout::id_of(&article.path),
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

/// This project's articles, or none for a project that has never had one. Each
/// subdirectory is one; a directory with no document in it is not.
pub fn list(project: &Path) -> Vec<Article> {
    migrate(project);
    let Ok(entries) = std::fs::read_dir(layout::dir(project)) else {
        return Vec::new();
    };
    let paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| layout::content(&entry.path()))
        .filter(|path| path.is_file())
        .collect();
    let mut articles: Vec<Article> = paths.into_iter().map(Article::new).collect();
    articles.sort_by_key(|article| Reverse(article.touched));
    articles
}

pub fn create(project: &Path) -> Option<Article> {
    let dir = layout::init(project).ok()?;
    let article = layout::free(&dir, schema::stamp::now());
    std::fs::create_dir_all(&article).ok()?;
    let path = layout::content(&article);
    std::fs::write(&path, "").ok()?;
    Some(Article::new(path))
}

/// Articles used to sit loose in `.cydonia/` as `foo.md` beside `foo.cover-N.svg`,
/// and the stem was the name the sidebar showed. Give each one a directory of
/// its own age, and keep that stem by writing it in as the title it was.
///
/// Runs the first time a project is opened after the change; one with nothing
/// loose in it costs the `read_dir` [`list`] was about to do anyway.
fn migrate(project: &Path) {
    let Ok(entries) = std::fs::read_dir(project::dir(project)) else {
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
        let to = layout::free(&layout::dir(project), schema::stamp::of(path));
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
