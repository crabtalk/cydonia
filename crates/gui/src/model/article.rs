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

use crate::model::language::Grammars as _;
use crate::model::{cover, file_url, language, store::Store, workspace::Workspace};
use artifact::location::Location;
use artifact::project::Project as _;
use artifact::{
    article as layout,
    article::{
        comments::{Author, Comment, Comments, Reply},
        properties::{self, CommentDisplay, Properties},
    },
};
use bezel::{
    gpui::{App, AppContext as _, Context, Entity},
    ui::input::{Shape, TextField},
};
use editor::{Anchor, AnchorId, Document, DocumentEvent, Mode, SourceAnchor};
use markdown::AppExt as _;
use std::{
    cell::OnceCell,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
};

/// What articles were called before they were named for their age, and what an
/// unnamed one was called among them.
const UNTITLED: &str = "untitled";

/// What a document with no title is shown as.
pub const UNNAMED: &str = "Untitled";

/// Claimed on the title field, so `enter` there moves to the body and stays a
/// newline in every other field.
pub const TITLE_CONTEXT: &str = "CydoniaArticleTitle";

/// An article's markdown as it is on disk, and the bytes each of its blocks
/// was parsed from — what a reference to part of it resolves against.
#[derive(Default)]
pub struct Disk {
    pub text: String,
    /// One per block, in order, partitioning `text` — see
    /// [`markdown::ParsedDoc::block_ranges`].
    pub blocks: Vec<Range<usize>>,
}

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
    /// The document every pane's editor on this article shares, once it has
    /// been opened.
    pub document: Option<Entity<Document>>,
    /// Which form the document opens in: the one it was last in, while no
    /// document is held.
    mode: Mode,
    /// What is on disk, as this process last wrote or read it.
    saved: String,
    /// [`Disk`], filled when first asked for and emptied whenever what is on
    /// disk changes.
    disk: OnceCell<Rc<Disk>>,
    /// How many words [`Article::saved`] holds, filled when first asked for
    /// and emptied with [`Article::disk`]. Read every frame the page is on.
    words: OnceCell<usize>,
    /// When the document was last written. Held rather than read back per
    /// frame: the sidebar orders on it, and a project of a thousand articles
    /// would be a thousand `stat` calls a frame.
    pub touched: u128,
    /// When it was made, where the backend knows — see
    /// [`layout::Article::created`].
    pub created: Option<u128>,
    /// Put away: listed under the divider rather than gone. Cached beside
    /// [`Article::touched`], and for the same reason.
    pub archived: bool,
    /// Set across the pane rather than in the reading column, and `None` for a
    /// page nobody has decided about — which follows the app's own default,
    /// see [`Article::wide`]. Cached like [`Article::archived`]: the frame
    /// reads it, and a frame is not somewhere to open a file.
    pub full_width: Option<bool>,
    /// How its margin shows its comments, and `None` for a page nobody has
    /// decided about — see [`Article::comments_shown`]. Cached like
    /// [`Article::full_width`].
    pub comment_display: Option<CommentDisplay>,
    /// Its labels, normalised — see [`artifact::label`]. Cached like
    /// [`Article::archived`].
    pub labels: Vec<String>,
    /// The start of its body as plain text — see [`Article::preview`].
    preview: OnceCell<String>,
    /// The file moved under an open document that has edits of its own — see
    /// [`Article::adopt`]. Runtime only: what it marks is a disagreement
    /// between the buffer and the disk, and reopening the app ends it by
    /// reading the disk.
    pub stale: bool,
    /// The notes in its margin, held against [`Article::saved`] while it is
    /// open.
    pub comments: Comments,
    /// The comment whose note is open, pressed in the margin or on its text.
    pub active_comment: Option<u64>,
    /// The comment whose note, or whose text, the pointer is over.
    pub hovered_comment: Option<u64>,
    /// The newest comment id read from the file. One there above it was filed
    /// since, and a write here keeps it.
    known_comment: u64,
}

impl Article {
    fn new(project: &Path, store: Store, held: layout::Article) -> Self {
        let properties = store.properties(&held.id);
        Self {
            number: held.number,
            path: layout::content(&layout::dir(project).join(&held.id)),
            cover: held.cover.as_ref().and_then(file_url::to_path),
            title: properties.title,
            touched: held.touched,
            created: held.created,
            archived: properties.archived,
            full_width: properties.full_width,
            comment_display: properties.comments,
            labels: properties.labels,
            preview: OnceCell::new(),
            id: held.id,
            store,
            field: None,
            document: None,
            mode: Mode::default(),
            saved: String::new(),
            disk: OnceCell::new(),
            words: OnceCell::new(),
            stale: false,
            comments: Comments::default(),
            active_comment: None,
            hovered_comment: None,
            known_comment: 0,
        }
    }

    /// What is held beside the markdown, as the backend files it.
    fn properties(&self) -> Properties {
        Properties {
            title: self.title.clone(),
            archived: self.archived,
            full_width: self.full_width,
            comments: self.comment_display,
            labels: self.labels.clone(),
        }
    }

    pub fn unload(&mut self, cx: &App) {
        if !self.archived || self.edited(cx) {
            return;
        }
        self.mode = self.mode(cx);
        self.field = None;
        self.document = None;
        self.saved = String::new();
    }

    /// Where the pictures in its body are filed.
    pub fn shelf(&self) -> crate::model::media::Shelf {
        crate::model::media::Shelf {
            store: self.store.clone(),
            owner: artifact::project::Owner::Article(self.id.clone()),
        }
    }

    pub fn archive(&mut self, archived: bool, cx: &App) {
        self.archived = archived;
        self.save_properties(cx);
    }

    /// Give the article exactly these labels.
    pub fn set_labels(&mut self, labels: Vec<String>, cx: &App) {
        self.labels = labels;
        self.save_properties(cx);
    }

    /// Write the properties back. A failed write is not reported.
    fn save_properties(&self, cx: &App) {
        cx.background_spawn(self.store.save_properties(&self.id, &self.properties()))
            .detach();
    }

    /// Set the page across the pane, or back in the column. `None` hands it
    /// back to the reader's default and takes the key out of the file.
    pub fn set_full_width(&mut self, wide: Option<bool>, cx: &App) {
        self.full_width = wide;
        self.save_properties(cx);
    }

    /// How wide this page is actually drawn, against the app's own default.
    /// One answer, so the pane and the menu that toggles it cannot disagree.
    pub fn wide(&self, default: bool) -> bool {
        self.full_width.unwrap_or(default)
    }

    /// Set how the margin shows its comments. `None` hands it back to the
    /// reader's default and takes the key out of the file.
    pub fn set_comment_display(&mut self, display: Option<CommentDisplay>, cx: &App) {
        self.comment_display = display;
        self.save_properties(cx);
    }

    /// How this page's margin shows its comments, against the app's own
    /// default.
    pub fn comments_shown(&self, default: CommentDisplay) -> CommentDisplay {
        self.comment_display.unwrap_or(default)
    }

    /// Which form the document is in. The document's answer where there is
    /// one: an editor switches it, and an undo can step back over a switch.
    pub fn mode(&self, cx: &App) -> Mode {
        match &self.document {
            Some(document) => document.read(cx).mode(),
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

    /// Put a field over the title and a document under the content.
    /// Idempotent — reopening an article is what keeps its undo history.
    pub fn open(&mut self, cx: &mut Context<Workspace>) {
        if self.document.is_some() {
            return;
        }
        // The document's own heading type, so the title is set the way the page
        // would set its own first heading.
        let h1 = cx.typography().h1;
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
        self.forget_disk();
        let (saved, mode) = (self.saved.clone(), self.mode);
        let document = cx.new(|cx| Document::new(&saved, cx).with_mode(mode));
        language::Here::ensure(fences(document.read(cx).doc()), cx);
        // Edits only: a selection drag and the caret blink notify too, and
        // each write serializes the whole document.
        cx.subscribe(
            &document,
            |workspace, document, event: &DocumentEvent, cx| {
                if !event.edited() {
                    return;
                }
                workspace.write_article(document.entity_id(), cx);
                // On every change rather than on open alone: a fence is usually
                // tagged after it is made, and the grammar is wanted the moment it
                // is named.
                language::Here::ensure(fences(document.read(cx).doc()), cx);
            },
        )
        .detach();

        self.field = Some(field);
        self.document = Some(document);
        self.comments = self.store.comments(&self.id);
        self.comments.carry(None, &self.saved);
        self.know_comments();
        self.place_comments(cx);
    }

    fn know_comments(&mut self) {
        let newest = self
            .comments
            .comments
            .iter()
            .map(|comment| comment.id)
            .max();
        self.known_comment = self.known_comment.max(newest.unwrap_or_default());
    }

    /// Write the comments beside `text`, the markdown on disk, with any filed
    /// there since they were read. Answers whether any were.
    fn save_comments(&mut self, text: &str, cx: &App) -> bool {
        let mut file = self.store.comments(&self.id);
        file.carry(None, text);
        let answered = self.filed_by_agent(&file);
        self.comments.take_replies(&file);
        let arrived: Vec<Comment> = file
            .comments
            .into_iter()
            .filter(|comment| {
                comment.id > self.known_comment
                    && !self
                        .comments
                        .comments
                        .iter()
                        .any(|held| held.id == comment.id)
            })
            .collect();
        let any = !arrived.is_empty();
        self.comments.comments.extend(arrived);
        self.know_comments();
        if let Some(id) = answered {
            self.active_comment = Some(id);
        }
        cx.background_spawn(self.store.save_comments(&self.id, &self.comments))
            .detach();
        any || answered.is_some()
    }

    /// The newest open comment an agent has just filed in `fresh`: a note it
    /// added, or an answer on one. What the margin opens on when it lands.
    fn filed_by_agent(&self, fresh: &Comments) -> Option<u64> {
        fresh
            .comments
            .iter()
            .filter(|comment| !comment.resolved)
            .filter(|comment| {
                match self
                    .comments
                    .comments
                    .iter()
                    .find(|held| held.id == comment.id)
                {
                    None => comment.id > self.known_comment,
                    Some(held) => comment.agent_replies() > held.agent_replies(),
                }
            })
            .map(|comment| comment.id)
            .max()
    }

    /// Hand the comments' ranges to the document, which carries them through
    /// edits from here on.
    fn place_comments(&self, cx: &mut App) {
        self.place_comments_over(&self.saved.clone(), cx);
    }

    /// [`Article::place_comments`] with the ranges into `text`, the markdown
    /// the document is.
    fn place_comments_over(&self, text: &str, cx: &mut App) {
        let Some(document) = &self.document else {
            return;
        };
        let anchors = self
            .comments
            .comments
            .iter()
            .map(|comment| SourceAnchor {
                id: AnchorId(comment.id),
                range: comment.range(),
                state: self.wash(comment),
                actionable: comment.opens(),
            })
            .collect();
        document.update(cx, |document, cx| {
            document.set_source_anchors(text, anchors, cx)
        });
    }

    /// Repaint the comments' washes over the ranges the document holds, and
    /// say again which of them a press opens.
    fn restate_comments(&self, cx: &mut App) {
        let Some(document) = &self.document else {
            return;
        };
        let anchors: Vec<Anchor> = document
            .read(cx)
            .anchors()
            .iter()
            .map(|anchor| {
                let held = self
                    .comments
                    .comments
                    .iter()
                    .find(|comment| comment.id == anchor.id.0);
                Anchor {
                    state: held.map_or(anchor.state, |comment| self.wash(comment)),
                    actionable: held.map_or(anchor.actionable, Comment::opens),
                    ..anchor.clone()
                }
            })
            .collect();
        if document.read(cx).anchors() != anchors.as_slice() {
            document.update(cx, |document, cx| document.set_anchors(anchors, cx));
        }
    }

    fn wash(&self, comment: &Comment) -> markdown::Annotation {
        if comment.resolved {
            markdown::Annotation::Resolved
        } else if [self.active_comment, self.hovered_comment].contains(&Some(comment.id)) {
            markdown::Annotation::Active
        } else {
            markdown::Annotation::Open
        }
    }

    /// Take the document's ranges into the comments, which hold them against
    /// `source`, what was just written, and write them beside it.
    fn write_comments(&mut self, source: &str, cx: &mut App) {
        let Some(document) = &self.document else {
            return;
        };
        // None held, none to lose: one filed meanwhile arrives through the
        // watch.
        if self.comments.comments.is_empty() {
            return;
        }
        let (placed, ranges) = document.read(cx).source_with_anchors();
        for comment in &mut self.comments.comments {
            if let Some(range) = ranges
                .iter()
                .find(|(id, _)| id.0 == comment.id)
                .and_then(|(_, range)| range.clone())
            {
                comment.start = range.start;
                comment.end = range.end;
            }
        }
        self.comments.stamp(&placed);
        self.comments.carry(Some(&placed), source);
        if self.save_comments(source, cx) {
            self.place_comments_over(source, cx);
        }
    }

    /// Read the comments again, for a file that moved beside a document that
    /// did not. Answers whether they changed.
    fn reread_comments(&mut self, cx: &mut App) -> bool {
        let mut fresh = self.store.comments(&self.id);
        fresh.carry(None, &self.saved);
        if fresh == self.comments {
            return false;
        }
        let answered = self.filed_by_agent(&fresh);
        self.comments = fresh;
        self.know_comments();
        if let Some(id) = answered {
            self.active_comment = Some(id);
        }
        self.place_comments(cx);
        true
    }

    /// Open one comment's note, or close the one open.
    pub fn activate_comment(&mut self, id: Option<u64>, cx: &mut App) {
        self.active_comment = id;
        self.restate_comments(cx);
    }

    pub fn hover_comment(&mut self, id: Option<u64>, cx: &mut App) {
        // A resolved comment's text is washed but has no note.
        let id = id.filter(|id| {
            self.comments
                .comments
                .iter()
                .any(|comment| comment.id == *id && comment.opens())
        });
        if self.hovered_comment != id {
            self.hovered_comment = id;
            self.restate_comments(cx);
        }
    }

    /// Mark one comment resolved, or open again.
    pub fn resolve_comment(&mut self, id: u64, resolved: bool, cx: &mut App) {
        let Some(comment) = self.comments.comments.iter_mut().find(|c| c.id == id) else {
            return;
        };
        comment.resolved = resolved;
        if resolved && self.active_comment == Some(id) {
            self.active_comment = None;
        }
        match self.save_comments(&self.saved.clone(), cx) {
            true => self.place_comments(cx),
            false => self.restate_comments(cx),
        }
    }

    /// Add the reader's follow-up `question` to comment `id`'s thread.
    pub fn ask_on_comment(&mut self, id: u64, question: String, cx: &mut App) {
        let Some(comment) = self.comments.get_mut(id) else {
            return;
        };
        comment.replies.push(Reply {
            from: Author::Reader,
            text: question,
            turn: None,
        });
        self.save_comments(&self.saved.clone(), cx);
        self.restate_comments(cx);
    }

    pub fn delete_comment(&mut self, id: u64, cx: &mut App) {
        self.comments.comments.retain(|comment| comment.id != id);
        if self.active_comment == Some(id) {
            self.active_comment = None;
        }
        self.save_comments(&self.saved.clone(), cx);
        self.place_comments(cx);
    }

    /// Best effort, like every other write here: a document that cannot be
    /// saved is not worth failing a keystroke over.
    ///
    /// Each surface writes its own file, so typing in the body never touches
    /// the properties and naming the page never touches the markdown. Answers
    /// whether the title moved, which is what the caller repaints on.
    pub fn write(&mut self, cx: &mut Context<Workspace>) -> bool {
        let renamed = match &self.field {
            Some(field) => {
                let title = field.read(cx).content().to_string();
                let moved = self.title != title;
                if moved {
                    self.title = title;
                    self.touched = artifact::stamp::now();
                    self.save_properties(cx);
                }
                moved
            }
            None => false,
        };
        if let Some(document) = &self.document {
            let source = document.read(cx).source();
            if self.saved != source {
                let written = self.store.write_article(&self.id, &source);
                let path = self.path.clone();
                cx.spawn(async move |workspace, cx| {
                    if written.await.is_err() {
                        return;
                    }
                    let _ = workspace.update(cx, |workspace, cx| {
                        if let Some(article) = workspace.article_at_mut(&path)
                            && article.wrote(source, cx)
                        {
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
        }
        renamed
    }

    /// Take `source` as what is on disk, once its write has landed: the
    /// comments go beside it. Answers whether the stale notice came down.
    fn wrote(&mut self, source: String, cx: &mut App) -> bool {
        self.write_comments(&source, cx);
        self.saved = source;
        self.forget_disk();
        self.touched = artifact::stamp::now();
        // The buffer is the file again, whatever landed under it while it was
        // not — typing on is the third answer to the notice, and it is the
        // one most people will give.
        std::mem::take(&mut self.stale)
    }

    /// Keep the buffer and write it over what landed on disk — the pane's other
    /// way out of the notice. The same write a keystroke makes, said out loud.
    pub fn keep(&mut self, cx: &mut Context<Workspace>) {
        self.write(cx);
        self.stale = false;
    }

    /// Take what a re-read of the project found — see [`crate::model::watch`].
    ///
    /// The file wins, except where the document is open with edits that have
    /// not been written. There the buffer stands and the pane is told the file
    /// moved underneath it: an agent's write and a half-typed paragraph are
    /// both somebody's work, and this is not the layer that gets to choose.
    pub fn adopt(&mut self, fresh: &Self, cx: &mut Context<Workspace>) {
        self.number = fresh.number;
        self.cover = fresh.cover.clone();
        self.archived = fresh.archived;
        self.full_width = fresh.full_width;
        self.comment_display = fresh.comment_display;
        self.labels = fresh.labels.clone();
        self.touched = fresh.touched;
        self.created = fresh.created;
        // Never opened: the label is the whole of what is held, and the file
        // is where it came from.
        if self.document.is_none() {
            self.title = fresh.title.clone();
            self.forget_disk();
            return;
        }
        // The echo of our own write, which every save produces. `saved` is what
        // this process last put on disk, so the two agreeing is the file saying
        // nothing new.
        let disk = self.store.read_article(&self.id).unwrap_or_default();
        if disk == self.saved && fresh.title == self.title {
            self.reread_comments(cx);
            return;
        }
        if self.edited(cx) {
            self.stale = true;
            return;
        }
        self.revert(cx);
    }

    /// Read the title and the document again from what is on disk. What the
    /// pane's Reload does, and what [`Article::adopt`] does for a document
    /// with nothing of its own to lose.
    ///
    /// The undo history goes: it is a history of a document this one no
    /// longer is.
    pub fn revert(&mut self, cx: &mut Context<Workspace>) {
        let held = self.store.properties(&self.id);
        self.title = held.title;
        if let Some(article) = self.store.article(&self.id) {
            self.touched = article.touched;
            self.cover = article.cover.as_ref().and_then(file_url::to_path);
        }
        self.archived = held.archived;
        self.full_width = held.full_width;
        self.comment_display = held.comments;
        self.labels = held.labels;
        let old = std::mem::take(&mut self.saved);
        self.saved = self.store.read_article(&self.id).unwrap_or_default();
        self.forget_disk();
        if let Some(field) = &self.field {
            let title = self.title.clone();
            field.update(cx, |field, cx| field.set_content(title, cx));
        }
        if let Some(document) = &self.document {
            let saved = self.saved.clone();
            document.update(cx, |document, cx| document.reload(&saved, cx));
        }
        // Through the change from what was written here, where the file still
        // holds the comments against it.
        let mut held = self.store.comments(&self.id);
        held.carry(Some(&old), &self.saved);
        self.comments = held;
        self.know_comments();
        cx.background_spawn(self.store.save_comments(&self.id, &self.comments))
            .detach();
        self.place_comments(cx);
        self.stale = false;
    }

    /// The markdown on disk and its blocks: what this process last wrote for an
    /// open article, and a read of the file for one never opened.
    /// The words in the open document as last saved: each run of letters or
    /// digits, and each CJK character on its own. `None` while it is not open.
    pub fn words(&self) -> Option<usize> {
        self.document.as_ref()?;
        Some(*self.words.get_or_init(|| {
            unicode_segmentation::UnicodeSegmentation::unicode_words(self.saved.as_str()).count()
        }))
    }

    /// Drop what is derived from the file: it has changed.
    fn forget_disk(&mut self) {
        self.disk = OnceCell::new();
        self.words = OnceCell::new();
        self.preview = OnceCell::new();
    }

    /// The start of its body as plain text — see [`artifact::text::preview`]:
    /// of what this process last wrote for an open article, and of the file,
    /// read once, for one never opened.
    pub fn preview(&self) -> &str {
        self.preview.get_or_init(|| match self.document {
            Some(_) => artifact::text::preview(&self.saved),
            None => artifact::text::preview(&self.store.read_article(&self.id).unwrap_or_default()),
        })
    }

    pub fn disk(&self) -> Rc<Disk> {
        self.disk
            .get_or_init(|| {
                let text = match self.document {
                    Some(_) => self.saved.clone(),
                    None => self.store.read_article(&self.id).unwrap_or_default(),
                };
                let blocks = markdown::parse_ranges(&text).block_ranges;
                Rc::new(Disk { text, blocks })
            })
            .clone()
    }

    /// Whether either surface holds something the disk does not. The title is
    /// filed on the keystroke, so in practice this is the body — but a rename
    /// that failed to write leaves the field ahead of the file too.
    fn edited(&self, cx: &App) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.read(cx).source() != self.saved)
            || self
                .field
                .as_ref()
                .is_some_and(|field| *field.read(cx).content() != self.title)
    }

    /// All of it: document, properties, cover and assets, and its number.
    pub fn remove(&self, cx: &App) {
        cx.background_spawn(self.store.remove_article(&self.id))
            .detach();
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
        let seed = cover::Seed::next(&self.path, self.cover.as_deref());
        let to = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .and_then(|ext| seed.path(&self.path, &ext));
        if let Some(to) = to.filter(|to| artifact::article::cover::import(source, to).is_ok()) {
            self.replace_cover(Some(to));
        }
    }

    /// A fresh generated cover. Adding one and shuffling are the same act —
    /// the first cover an article is given is already a throw.
    pub fn shuffle_cover(&mut self) {
        let seed = cover::Seed::next(&self.path, self.cover.as_deref());
        let Some(to) = seed.path(&self.path, "svg") else {
            return;
        };
        if std::fs::write(&to, seed.svg()).is_ok() {
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
            number: article.number,
            touched: article.touched,
            created: article.created,
            cover: article.cover.as_deref().and_then(file_url::from_path),
            labels: article.labels.clone(),
        }
    }
}

/// The articles of the project at `location`, kept in `store`, most recently
/// touched first. A project on this machine is migrated first.
pub fn list(location: &Location, store: &Store) -> Vec<Article> {
    if let Some(root) = location.local() {
        migrate(root);
    }
    store
        .articles()
        .into_iter()
        .map(|held| Article::new(location.root(), store.clone(), held))
        .collect()
}

/// A new, empty document in the project at `root`, kept in `store`, once the
/// backend has made it.
pub fn create(root: &Path, store: Store) -> impl Future<Output = Option<Article>> + use<> {
    let root = root.to_owned();
    let made = store.create_article("");
    async move {
        let held = made.await.ok()?;
        Some(Article::new(&root, store, held))
    }
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
pub(crate) fn fences(doc: &markdown::Doc) -> Vec<String> {
    doc.blocks
        .iter()
        .filter_map(|block| match &block.kind {
            markdown::BlockKind::Code { language, .. } => language.clone(),
            _ => None,
        })
        .collect()
}
