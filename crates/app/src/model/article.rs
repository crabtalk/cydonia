//! A project's articles: markdown documents in its `.cydonia/` directory.
//!
//! Files in the project rather than rows in a config file, unlike
//! [`crate::model::board`] — an article is written to be read by the agent
//! running in that directory, and a path is how it gets handed over.
//!
//! The path is the whole identity. A new article is `untitled.md` and takes
//! its name from its title the first time it is left; after that it stays put,
//! because by then something may have been pointed at it.

use crate::model::{project, workspace::Workspace};
use bezel::gpui::{AppContext as _, Context, Entity, ScrollHandle};
use editor::Editor;
use std::path::{Path, PathBuf};

/// What a document is called before it says.
const UNTITLED: &str = "untitled";

/// How long a name derived from a heading is allowed to get.
const SLUG_MAX: usize = 48;

pub struct Article {
    pub path: PathBuf,
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
            path,
            editor: None,
            scroll: ScrollHandle::new(),
            saved: String::new(),
        }
    }

    /// The rail's label.
    pub fn title(&self) -> String {
        self.path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Read the file and put an editor over it. Idempotent — reopening an
    /// article is what keeps its undo history and its scroll.
    pub fn open(&mut self, cx: &mut Context<Workspace>) {
        if self.editor.is_some() {
            return;
        }
        self.saved = std::fs::read_to_string(&self.path).unwrap_or_default();
        let scroll = self.scroll.clone();
        let editor = cx.new(|cx| Editor::new(&self.saved, cx).with_scroll(scroll));
        cx.observe(&editor, move |workspace, editor, cx| {
            workspace.write_article(editor.entity_id(), editor.read(cx).source());
        })
        .detach();
        self.editor = Some(editor);
    }

    /// Best effort, like every other write here: a document that cannot be
    /// saved is not worth failing a keystroke over.
    pub fn write(&mut self, source: String) {
        if self.saved == source || std::fs::write(&self.path, &source).is_err() {
            return;
        }
        self.saved = source;
    }

    pub fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
    }

    /// Take the name the document gives itself, once. Run when the article is
    /// left rather than when it is saved: a save happens on every keystroke,
    /// and `# D` on the way to `# Design notes` is not a title.
    ///
    /// Only while the file is still untitled — after that it is a path
    /// something may have been pointed at, and it stays where it is.
    pub fn rename(&mut self) {
        if !self.title().starts_with(UNTITLED) {
            return;
        }
        let Some(slug) = self.saved.lines().next().and_then(slug) else {
            return;
        };
        let to = self.path.with_file_name(format!("{slug}.md"));
        if to.exists() || std::fs::rename(&self.path, &to).is_err() {
            return;
        }
        self.path = to;
    }
}

/// This project's articles, or none for a project that has never had one.
pub fn list(project: &Path) -> Vec<Article> {
    let Ok(entries) = std::fs::read_dir(project::dir(project)) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths.into_iter().map(Article::new).collect()
}

pub fn create(project: &Path) -> Option<Article> {
    let dir = project::init(project).ok()?;
    let mut path = dir.join(format!("{UNTITLED}.md"));
    for n in 2.. {
        if !path.exists() {
            break;
        }
        path = dir.join(format!("{UNTITLED}-{n}.md"));
    }
    std::fs::write(&path, "").ok()?;
    Some(Article::new(path))
}

/// A file name from the document's first line: its block marks dropped, and
/// what is left reduced to what reads well in a path.
fn slug(line: &str) -> Option<String> {
    let mut slug = String::new();
    for ch in line.trim_start_matches(['#', '>', ' ']).chars() {
        if ch.is_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= SLUG_MAX {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    (!slug.is_empty()).then_some(slug)
}
