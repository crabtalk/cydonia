use crate::{
    model::workspace::{
        Showing, Workspace,
        references::{self, Part},
    },
    view::{
        canvas::image,
        component::file::{
            self,
            pictures::{Assets, Pictures},
        },
        kit::{icon, link, text::KindName as _},
    },
};
use artifact::{article::anchor, kind::Kind};
use bezel::{
    gpui::{
        self, App, AppContext as _, Context, Entity, Render, SharedString, Subscription, Task,
        Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::icons,
};
use canvas::{
    kind::{Card, Content, Look},
    model::Node,
};
use markdown::{AppExt as _, Editing};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

struct Embed {
    node: Node,
    root: PathBuf,
    workspace: Entity<Workspace>,
    document: Option<Entity<editor::Document>>,
    pictures: Entity<Pictures>,
    card: Card,
    parsed: markdown::Doc,
    source: String,
    error: Option<String>,
    _watch: Vec<Subscription>,
    _poll: Option<Task<()>>,
}

pub(crate) fn render(
    node: &Node,
    look: Look,
    root: &Path,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    if let Some(editor) = look.editor {
        return look
            .card
            .render(node, None, Content::Flow(editor), window, cx);
    }
    if node.file.as_deref().is_some_and(markdown::is_image) {
        return image::render(node, &look.card, root, window, cx);
    }
    if let Some(file) = &node.file {
        let path = root.join(file);
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let text = look.card.typography.body.size();
            let extension = path
                .extension()
                .map(|ext| format!(".{} file", ext.to_string_lossy()))
                .unwrap_or_else(|| "File".to_owned());
            let body = div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12. * look.card.zoom))
                .child(
                    icons::icon(icons::files::File)
                        .size(px(32. * look.card.zoom))
                        .text_color(Theme::of(cx).text_muted),
                )
                .child(
                    div()
                        .text_size(px(text))
                        .line_height(px(text * 1.5))
                        .child(name.into_owned()),
                )
                .child(
                    div()
                        .text_size(px(text * 0.8125))
                        .text_color(Theme::of(cx).text_muted)
                        .child(extension),
                )
                .into_any_element();
            return look
                .card
                .render(node, None, Content::Fill(body), window, cx);
        }
    }
    let Some(workspace) = references::current(cx) else {
        return div().into_any_element();
    };
    let target = node
        .url
        .as_ref()
        .or(node.file.as_ref())
        .cloned()
        .unwrap_or_default();
    let key: SharedString = format!(
        "canvas-source-{}-{target}-{}",
        node.id,
        node.subpath.as_deref().unwrap_or_default()
    )
    .into();
    let node = node.clone();
    let seed = node.clone();
    let root = root.to_path_buf();
    let card = look.card.clone();
    let view = window.use_keyed_state(key, cx, |_, cx| Embed::new(seed, root, workspace, card, cx));
    view.update(cx, |view, _| {
        view.node = node;
        view.card = look.card;
    });
    view.into_any_element()
}

impl Embed {
    fn new(
        node: Node,
        root: PathBuf,
        workspace: Entity<Workspace>,
        card: Card,
        cx: &mut Context<Self>,
    ) -> Self {
        let target = node.url.as_deref().unwrap_or_default();
        let resolved = workspace.read(cx).resolve(target, cx).ok();
        let document = if let Some(resolved) = &resolved
            && let Showing::Article(id) = &resolved.showing
        {
            workspace.update(cx, |workspace, cx| {
                let project = workspace
                    .projects
                    .iter_mut()
                    .find(|project| project.id == resolved.project)?;
                let article = project
                    .articles
                    .iter_mut()
                    .find(|article| &article.id == id)?;
                article.open(cx);
                article.document.clone()
            })
        } else {
            None
        };
        let base = resolved
            .as_ref()
            .and_then(|resolved| {
                let Showing::Article(id) = &resolved.showing else {
                    return None;
                };
                workspace
                    .read(cx)
                    .project(resolved.project)?
                    .articles
                    .iter()
                    .find(|article| &article.id == id)?
                    .path
                    .parent()
                    .map(Path::to_path_buf)
            })
            .unwrap_or_else(|| {
                node.file
                    .as_ref()
                    .and_then(|file| root.join(file).parent().map(Path::to_path_buf))
                    .unwrap_or(root.clone())
            });
        let pictures = cx.new(|cx| {
            Pictures::new(
                base.clone(),
                Assets::Dir(base.join("assets")),
                workspace.downgrade(),
                cx,
            )
        });
        let mut watch = vec![
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.observe(&pictures, |_, _, cx| cx.notify()),
        ];
        if let Some(document) = &document {
            watch.push(cx.observe(document, |_, _, cx| cx.notify()));
        }
        if let Some(resolved) = &resolved {
            match &resolved.showing {
                Showing::Session(id) => {
                    if let Some(session) = workspace.read(cx).session_entity(*id).cloned() {
                        watch.push(cx.observe(&session, |_, _, cx| cx.notify()));
                    }
                }
                Showing::Board(id) => {
                    if let Some(board) = workspace
                        .read(cx)
                        .project(resolved.project)
                        .and_then(|project| project.boards.iter().find(|held| held.id == *id))
                        .map(|held| held.board.clone())
                    {
                        watch.push(cx.observe(&board, |_, _, cx| cx.notify()));
                    }
                }
                Showing::Canvas(id) => {
                    if let Some(canvas) = workspace
                        .read(cx)
                        .project(resolved.project)
                        .and_then(|project| project.canvases.iter().find(|held| held.id == *id))
                        .map(|held| held.canvas.clone())
                    {
                        watch.push(cx.observe(&canvas, |_, _, cx| cx.notify()));
                    }
                }
                _ => {}
            }
        }
        let poll = node.file.as_ref().map(|_| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                    if this
                        .update(cx, |this, cx| {
                            if this.read_file() {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        let mut this = Self {
            node,
            root,
            workspace,
            document,
            pictures,
            card,
            parsed: markdown::Doc::default(),
            source: String::new(),
            error: None,
            _watch: watch,
            _poll: poll,
        };
        this.read_file();
        this
    }

    fn read_file(&mut self) -> bool {
        let Some(file) = &self.node.file else {
            return false;
        };
        let path = self.root.join(file);
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            return false;
        }
        let (source, error) = match file::read_text(&path) {
            Ok(source) => (source, None),
            Err(error) => (String::new(), Some(error.to_string())),
        };
        let changed = self.source != source || self.error != error;
        if changed {
            self.parsed = markdown::parse(&source);
            self.source = source;
            self.error = error;
        }
        changed
    }
}

impl Render for Embed {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut title = self.node.file.clone().unwrap_or_default();
        let mut source = self.source.clone();
        let mut mark = icons::files::FileText.into();
        let mut base = self
            .node
            .file
            .as_ref()
            .and_then(|file| self.root.join(file).parent().map(Path::to_path_buf));
        if let Some(document) = &self.document {
            source = document.read(cx).source();
        }
        if let Some(url) = &self.node.url {
            match self.workspace.read(cx).resolve(url, cx) {
                Ok(resolved) => {
                    title = if resolved.title.is_empty() {
                        resolved.kind.untitled().to_owned()
                    } else {
                        resolved.title
                    };
                    mark = icon::kind_icon(resolved.kind);
                    if resolved.kind != Kind::Article {
                        source = self
                            .workspace
                            .read(cx)
                            .project(resolved.project)
                            .and_then(|project| {
                                project
                                    .ix_of(&resolved.showing)
                                    .map(|ix| project.preview_at(resolved.kind, ix, cx))
                            })
                            .unwrap_or_default();
                        if source.is_empty() {
                            source = format!("{} · #{}", resolved.kind.label(), resolved.number);
                        }
                    }
                    match resolved.part {
                        Some(Part::Passage(passage)) => source = passage.markdown().to_owned(),
                        Some(Part::Card(card)) => source = card.text,
                        _ => {}
                    }
                    if let Showing::Article(id) = &resolved.showing {
                        base = self
                            .workspace
                            .read(cx)
                            .project(resolved.project)
                            .and_then(|project| {
                                project.articles.iter().find(|article| &article.id == id)
                            })
                            .and_then(|article| article.path.parent().map(Path::to_path_buf));
                    }
                }
                Err(error) => source = error,
            }
        }
        if let Some(subpath) = self
            .node
            .subpath
            .as_deref()
            .and_then(|path| path.strip_prefix('#'))
        {
            source = anchor::section(&source, subpath)
                .map(|(_, range)| source[range].to_owned())
                .unwrap_or_else(|| format!("Heading not found: {subpath}"));
        }
        if source != self.source {
            self.parsed = markdown::parse_with(&source, &cx.marks());
            self.source = source;
        }
        let body = link::nested(cx, |cx| {
            markdown::render_with(
                &self.parsed,
                Editing {
                    base: base.as_deref(),
                    typography: Some(self.card.typography),
                    image_overlay: Some(Pictures::overlay(&self.pictures)),
                    ..Default::default()
                },
                window,
                cx,
            )
        });
        let body = div()
            .child(body)
            .children(self.error.clone().map(|error| {
                div()
                    .text_style_at(TextStyle::Caption, self.card.zoom)
                    .child(error)
            }))
            .into_any_element();
        self.card.render(
            &self.node,
            Some((title, mark)),
            Content::Flow(body),
            window,
            cx,
        )
    }
}
