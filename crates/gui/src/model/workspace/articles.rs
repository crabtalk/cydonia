//! The articles a project holds, and the two surfaces each one opens.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;

impl Workspace {
    /// A fresh document in the active project, opened as it lands — an empty
    /// article has nothing to look at but the caret.
    pub fn new_article(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let project = self.active?;
        let article = article::create(&self.projects[project].path)?;
        // Where a re-read would put it: the list is newest first, and a new one
        // appended would sit at the bottom until the next load moved it.
        self.projects[project].articles.insert(0, article);
        self.reveal_project(project, cx);
        self.open_article(project, 0, cx);
        Some(0)
    }

    pub fn archive_article(&mut self, path: &Path, archived: bool, cx: &mut Context<Self>) {
        let Some(article) = self.article_at_mut(path) else {
            return;
        };
        article.archive(archived);
        self.prune_archived(cx);
        cx.notify();
    }

    /// The article a file names, wherever it is open — what the sidebar
    /// addresses one by, since an index moves when a neighbour is made.
    pub fn article_at_mut(&mut self, path: &Path) -> Option<&mut Article> {
        self.projects
            .iter_mut()
            .flat_map(|open| open.articles.iter_mut())
            .find(|article| article.path == path)
    }

    /// Every project's articles are on show, so picking one brings its project
    /// forward with it.
    pub fn open_article(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let text_size = self.article_font_size();
        let Some(article) = self
            .projects
            .get_mut(project)
            .and_then(|open| open.articles.get_mut(ix))
        else {
            return;
        };
        article.open(text_size, cx);
        self.projects[project].article = Some(ix);
        let id = self.projects[project].articles[ix]
            .path
            .to_string_lossy()
            .into_owned();
        self.active = Some(project);
        self.remember(project, state::Kind::Article, id, cx);
        cx.notify();
    }

    /// Move an article, its folder and assets with it, into another open
    /// project. Its buffer is written first, so what moves is what is on
    /// screen. Both projects are read again afterwards.
    pub fn move_article(&mut self, from: usize, ix: usize, to: usize, cx: &mut Context<Self>) {
        if from == to {
            return;
        }
        let (Some(source), Some(target)) = (
            self.projects.get(from).map(|open| open.path.clone()),
            self.projects.get(to).map(|open| open.path.clone()),
        ) else {
            return;
        };
        let Some(article) = self
            .projects
            .get_mut(from)
            .and_then(|open| open.articles.get_mut(ix))
        else {
            return;
        };
        article.write(cx);
        let content = article.path.clone();
        if artifact::article::move_to(&content, &target).is_err() {
            return;
        }
        self.reload_project(&source, cx);
        self.reload_project(&target, cx);
    }

    /// Drop the article: the file goes with it.
    pub fn delete_article(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        if ix >= project.articles.len() {
            return;
        }
        project.articles.remove(ix).remove();
        project.article = project
            .article
            .filter(|open| *open != ix)
            .map(|open| if open > ix { open - 1 } else { open });
        cx.notify();
    }

    /// Take the file over the buffer, for an article the watch found had moved
    /// underneath one — what the pane's notice offers. See [`Article::revert`].
    pub fn revert_article(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(article) = self.article_at_mut(path) else {
            return;
        };
        article.revert(cx);
        cx.notify();
    }

    /// Keep the buffer instead, and write it over what landed — the other half
    /// of the same notice. See [`Article::keep`].
    pub fn keep_article(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(article) = self.article_at_mut(path) else {
            return;
        };
        article.keep(cx);
        cx.notify();
    }

    pub fn active_article(&self) -> Option<&Article> {
        let project = self.active_project()?;
        project.articles.get(project.article?)
    }

    /// Put a cover on one article, or take it off — see [`Article::set_cover`].
    pub fn set_cover(&mut self, on: &Path, source: Option<&Path>, cx: &mut Context<Self>) {
        if let Some(article) = self.article_at_mut(on) {
            article.set_cover(source);
            cx.notify();
        }
    }

    /// Set one page across the pane, or back in the column — see
    /// [`Article::set_full_width`].
    pub fn set_full_width(&mut self, on: &Path, wide: Option<bool>, cx: &mut Context<Self>) {
        if let Some(article) = self.article_at_mut(on) {
            article.set_full_width(wide);
            cx.notify();
        }
    }

    /// Put one document into the other form — see [`Article::set_mode`].
    pub fn set_article_mode(&mut self, on: &Path, mode: Mode, cx: &mut Context<Self>) {
        if let Some(article) = self.article_at_mut(on) {
            article.set_mode(mode, cx);
            cx.notify();
        }
    }

    /// Cut one article a new cover — see [`Article::shuffle_cover`].
    pub fn shuffle_cover(&mut self, on: &Path, cx: &mut Context<Self>) {
        if let Some(article) = self.article_at_mut(on) {
            article.shuffle_cover();
            cx.notify();
        }
    }

    /// The title or the content changed. Found by the entity because an article
    /// has two surfaces and either can be the one that moved.
    pub fn write_article(&mut self, changed: EntityId, cx: &mut Context<Self>) {
        let found = self.projects.iter_mut().find_map(|project| {
            project.articles.iter_mut().find(|article| {
                article
                    .field
                    .as_ref()
                    .is_some_and(|field| field.entity_id() == changed)
                    || article
                        .editor
                        .as_ref()
                        .is_some_and(|editor| editor.entity_id() == changed)
            })
        });
        if found.is_some_and(|article| article.write(cx)) {
            cx.notify();
        }
    }
}
