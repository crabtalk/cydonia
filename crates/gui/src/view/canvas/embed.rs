use crate::{
    model::workspace::{
        Showing, Workspace,
        references::{self, Part},
    },
    view::{
        component::file::{
            self,
            pictures::{Assets, Pictures},
        },
        kit::link,
    },
};
use artifact::article::anchor;
use bezel::{
    gpui::{
        self, App, AppContext as _, Context, Entity, Render, SharedString, Subscription, Task,
        Window, div, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::scroll,
};
use canvas::{
    kind::{self, Chrome, Look},
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
    zoom: f32,
    selected: bool,
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
        return kind::chrome(Chrome::Card, node, look.zoom, cx)
            .child(editor)
            .into_any_element();
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
    let root = root.to_path_buf();
    let view = window.use_keyed_state(key, cx, |_, cx| Embed::new(node, root, workspace, cx));
    view.update(cx, |view, _| {
        view.zoom = look.zoom;
        view.selected = look.selected;
    });
    kind::chrome(Chrome::Card, &view.read(cx).node, look.zoom, cx)
        .child(view)
        .into_any_element()
}

impl Embed {
    fn new(
        node: Node,
        root: PathBuf,
        workspace: Entity<Workspace>,
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
            zoom: 1.,
            selected: false,
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
                    title = resolved.title;
                    if let Some(Part::Passage(passage)) = resolved.part {
                        source = passage.markdown().to_owned();
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
        let image = self
            .node
            .file
            .as_ref()
            .is_some_and(|file| markdown::is_image(file));
        if image {
            let file = Path::new(self.node.file.as_deref().unwrap())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            source = format!("![{file}](<{}>)", file);
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
                    typography: Some(cx.typography().scaled(self.zoom)),
                    image_overlay: Some(Pictures::overlay(&self.pictures)),
                    ..Default::default()
                },
                window,
                cx,
            )
        });
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(6. * self.zoom))
            .child(
                div()
                    .text_style_at(TextStyle::Caption, self.zoom)
                    .text_color(Theme::of(cx).text_muted)
                    .text_ellipsis()
                    .child(title),
            )
            .child(
                scroll::claiming_pane(
                    SharedString::from(format!("canvas-reference-scroll-{}", self.node.id)),
                    scroll::Axes::Vertical,
                    window,
                    cx,
                )
                .flex_1()
                .min_h_0()
                .when(!self.selected, |pane| pane.overflow_hidden())
                .child(body),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_style(TextStyle::Caption).child(error)),
            )
    }
}
