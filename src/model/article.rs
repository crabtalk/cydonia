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

use crate::model::{cover, project, properties, workspace::Workspace};
use bezel::{
    gpui::{App, AppContext as _, Context, Entity, ScrollHandle},
    ui::input::{Shape, TextField},
};
use editor::Editor;
use markdown::Typography;
use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// What articles were called before they were named for their age, and what an
/// unnamed one was called among them.
const UNTITLED: &str = "untitled";

/// What a document with no title is shown as.
pub const UNNAMED: &str = "Untitled";

/// Claimed on the title field, so `enter` there moves to the body and stays a
/// newline in every other field.
pub const TITLE_CONTEXT: &str = "CydoniaArticleTitle";

/// Where a project's articles live, and what the document is called inside the
/// directory that is one.
const DIR: &str = "articles";
const CONTENT: &str = "content.md";

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
}

impl Article {
    fn new(path: PathBuf) -> Self {
        Self {
            cover: cover::of(&path),
            title: properties::title(&path),
            path,
            field: None,
            editor: None,
            scroll: ScrollHandle::new(),
            saved: String::new(),
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
            }
        }
        renamed
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

/// This project's articles, or none for a project that has never had one. Each
/// subdirectory is one; a directory with no document in it is not.
pub fn list(project: &Path) -> Vec<Article> {
    let dir = project::dir(project);
    migrate(&dir);
    let Ok(entries) = std::fs::read_dir(dir.join(DIR)) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path().join(CONTENT))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    paths.into_iter().map(Article::new).collect()
}

pub fn create(project: &Path) -> Option<Article> {
    let dir = project::init(project).ok()?.join(DIR);
    let article = free(&dir, project::stamp());
    std::fs::create_dir_all(&article).ok()?;
    let path = article.join(CONTENT);
    std::fs::write(&path, "").ok()?;
    Some(Article::new(path))
}

/// This millisecond's directory, or the first after it that is not taken. Two
/// articles made inside one millisecond is the only way that happens.
fn free(dir: &Path, stamp: u128) -> PathBuf {
    (stamp..)
        .map(|stamp| dir.join(stamp.to_string()))
        .find(|article| !article.exists())
        .unwrap_or_else(|| dir.join(stamp.to_string()))
}

/// When this file was last written, for an article being given the id it should
/// have been made with.
fn written(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(project::stamp, |since| since.as_millis())
}

/// Articles used to sit loose in `.cydonia/` as `foo.md` beside `foo.cover-N.svg`,
/// and the stem was the name the sidebar showed. Give each one a directory of
/// its own age, and keep that stem by writing it in as the title it was.
///
/// Runs the first time a project is opened after the change; one with nothing
/// loose in it costs the `read_dir` [`list`] was about to do anyway.
fn migrate(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
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
        let to = free(&dir.join(DIR), written(path));
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
        let content = to.join(CONTENT);
        if std::fs::rename(path, &content).is_ok() && !stem.starts_with(UNTITLED) {
            // Verbatim, slug and all: it is what the sidebar was already
            // showing, so nothing a person is looking at changes.
            properties::set_title(&content, stem);
        }
    }
}
