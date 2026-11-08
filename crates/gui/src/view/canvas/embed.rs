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
        kit::{icon, link, text::KindName as _},
    },
};
use artifact::{article::anchor, kind::Kind};
use bezel::{
    gpui::{
        self, App, AppContext as _, Context, Entity, ObjectFit, Render, SharedString, Subscription,
        Task, Window, div, img, prelude::*, px,
    },
    theme::{TextStyle, Theme, Typeset as _},
    ui::{icons, scroll},
};
use canvas::{
    kind::{self, Chrome, Look},
    model::Node,
    presentation::{self, Detail, Measure},
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
    detail: Detail,
    measure: Option<Measure>,
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
    if let Some(file) = &node.file {
        let path = root.join(file);
        if !markdown::is_image(file)
            && !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if look.detail == Detail::Full {
                let text = presentation::typography(cx, look.zoom).body.size();
                let extension = path
                    .extension()
                    .map(|ext| format!(".{} file", ext.to_string_lossy()))
                    .unwrap_or_else(|| "File".to_owned());
                return kind::chrome(Chrome::Card, node, look.zoom, cx)
                    .justify_center()
                    .gap(px(12. * look.zoom))
                    .child(
                        icons::icon(icons::files::File)
                            .size(px(32. * look.zoom))
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
            }
            return kind::summary(
                node,
                &name,
                Some(icons::files::File.into()),
                look.detail,
                look.zoom,
                cx,
            );
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
    let view = window.use_keyed_state(key, cx, |_, cx| Embed::new(seed, root, workspace, cx));
    view.update(cx, |view, _| {
        view.node = node;
        view.zoom = look.zoom;
        view.selected = look.selected;
        view.detail = look.detail;
        view.measure = Some(look.measure);
    });
    view.into_any_element()
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
            zoom: 1.,
            selected: false,
            detail: Detail::Full,
            measure: None,
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
        if let Some(file) = self
            .node
            .file
            .as_ref()
            .filter(|file| markdown::is_image(file))
        {
            let path = self.root.join(file);
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let overlay = Pictures::overlay(&self.pictures);
            let tools = self
                .selected
                .then(|| overlay(0, &name, window, cx))
                .flatten();
            return kind::chrome(Chrome::Bare, &self.node, self.zoom, cx)
                .relative()
                .overflow_hidden()
                .child(img(path.clone()).size_full().object_fit(ObjectFit::Contain))
                .children(
                    tools.map(|tools| div().absolute().bottom(px(4.)).right(px(4.)).child(tools)),
                )
                .into_any_element();
        }
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
        if self.detail != Detail::Full {
            return kind::summary(&self.node, &title, Some(mark), self.detail, self.zoom, cx);
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
                    typography: Some(presentation::typography(cx, self.zoom)),
                    image_overlay: Some(Pictures::overlay(&self.pictures)),
                    ..Default::default()
                },
                window,
                cx,
            )
        });
        let metadata = presentation::typography(cx, self.zoom).body.size() * 13. / 16.;
        let header = div()
            .flex()
            .items_center()
            .gap(px(6. * self.zoom))
            .text_size(px(metadata))
            .line_height(px(metadata * 18. / 13.))
            .text_color(Theme::of(cx).text_muted)
            .child(icons::icon(mark).size(px(14. * self.zoom)))
            .child(div().min_w_0().text_ellipsis().child(title));
        let padding = kind::PAD * 2. + 8. + metadata / self.zoom * 18. / 13. + 2. / self.zoom;
        let body = match &self.measure {
            Some(measure) => measure.wrap(body, padding),
            None => body,
        };
        kind::chrome(Chrome::Card, &self.node, self.zoom, cx)
            .min_h_0()
            .gap(px(8. * self.zoom))
            .child(header)
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
            .into_any_element()
    }
}
