use crate::{
    model::workspace::references,
    view::{
        canvas::{CanvasView, embed, image},
        component::file::external,
        window::reach::InWindow,
    },
};
use artifact::reference;
use bezel::gpui::{Context, PathPromptOptions, Window};
use canvas::{
    Change, Kinds, kind,
    model::{self, Node},
};
use markdown::{Form, Link, render::Bookmark};
use std::path::PathBuf;
use url::Url;

pub(crate) fn kinds(kinds: Kinds, root: PathBuf) -> Kinds {
    let file_root = root.clone();
    let controls_root = root.clone();
    let file = kind::Kind::new(move |node, look, window, cx| {
        embed::render(node, look, &file_root, window, cx)
    })
    .controls(move |node, card, window, cx| {
        image::Opener::for_node(node, card, &controls_root, window, cx)
    })
    .edit(
        kind::Field::new(
            |node| {
                format!(
                    "{}{}",
                    node.file.as_deref().unwrap_or_default(),
                    node.subpath.as_deref().unwrap_or_default()
                )
            },
            |node, text| match text.split_once('#') {
                Some((file, subpath)) => {
                    node.file = Some(file.into());
                    node.subpath = Some(format!("#{subpath}"));
                }
                None => {
                    node.file = Some(text);
                    node.subpath = None;
                }
            },
        )
        .with_control(|_| kind::Control::External),
    )
    .exportable()
    .handles(canvas::handle::frame)
    .open_in_host();
    let original = kinds.get(model::LINK).clone().open_in_host();
    let mut links = original;
    if let Some(field) = &mut links.rules.edit {
        field.control = |node| {
            if node
                .url
                .as_deref()
                .is_some_and(|url| url.starts_with(reference::SCHEME))
            {
                kind::Control::External
            } else {
                kind::Control::Url
            }
        };
    }
    let root = PathBuf::new();
    let render = links.render.clone();
    links.render = std::rc::Rc::new(move |node, look, window, cx| {
        if node
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with(reference::SCHEME))
        {
            embed::render(node, look, &root, window, cx)
        } else if look.editor.is_none()
            && node.url.as_deref().is_some_and(|url| {
                Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            })
        {
            Bookmark {
                link: Link::new(node.url.as_deref().unwrap_or_default(), None),
                form: Form::Embed(None),
                typography: look.card.typography,
                height: Some(look.card.size.height),
            }
            .render(cx)
        } else {
            render(node, look, window, cx)
        }
    });
    kinds.with(model::FILE, file).with(model::LINK, links)
}

impl CanvasView {
    pub(crate) fn pick_files(&mut self, replace: Option<String>, cx: &mut Context<Self>) {
        if self.surface.read(cx).editor().options().read_only {
            return;
        }
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: replace.is_none(),
            prompt: Some("Add files".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.add_files(paths, replace, cx));
        })
        .detach();
    }

    pub(crate) fn add_files(
        &mut self,
        paths: Vec<PathBuf>,
        replace: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.surface.read(cx).editor().options().read_only {
            return;
        }
        let at = self.at;
        let root = self.path.clone();
        self.surface.update(cx, |surface, cx| {
            surface.update_editor(cx, |editor| {
                let mut changes = Vec::new();
                let ids = editor.canvas().mint_n(paths.len());
                for (index, (path, id)) in paths.into_iter().zip(ids).enumerate() {
                    let file = path
                        .strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    if let Some(node) = replace
                        .as_ref()
                        .and_then(|id| editor.canvas().node(id))
                        .cloned()
                    {
                        let mut node = node;
                        node.kind = model::FILE.into();
                        node.file = Some(file);
                        node.url = None;
                        node.subpath = None;
                        changes.push(Change::UpdateNode { node });
                        break;
                    }
                    changes.push(Change::AddNode {
                        node: Node {
                            id,
                            kind: model::FILE.into(),
                            x: at.0 + index as i64 * 24,
                            y: at.1 + index as i64 * 24,
                            width: 360,
                            height: 260,
                            file: Some(file),
                            ..Default::default()
                        },
                        index: None,
                    });
                }
                editor.apply(changes);
            })
        });
    }

    pub(crate) fn reference(
        &mut self,
        text: &str,
        replace: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.surface.read(cx).editor().options().read_only {
            return Err("canvas is read-only".into());
        }
        let workspace = references::current(cx).ok_or("workspace closed")?;
        let workspace = workspace.read(cx);
        let home = workspace
            .project(self.canvas.project)
            .ok_or("project closed")?;
        let mut named = reference::parse(text).ok_or("Choose an entry or enter its #number")?;
        let name = reference::project_name(&home.path).ok_or("project has no directory name")?;
        if named.project.is_none() {
            named.project = Some(&name);
        }
        let url = named.link();
        workspace.resolve(&url, cx)?;
        let mut node = replace
            .as_ref()
            .and_then(|id| self.surface.read(cx).editor().canvas().node(id))
            .cloned()
            .unwrap_or(Node {
                kind: model::LINK.into(),
                width: 360,
                height: 260,
                ..Default::default()
            });
        node.kind = model::LINK.into();
        node.url = Some(url);
        node.file = None;
        node.subpath = None;
        self.surface.update(cx, |surface, cx| {
            surface.update_editor(cx, |editor| {
                if replace.is_some() {
                    editor.apply([Change::UpdateNode { node }]);
                } else {
                    editor.add_node(node, self.at);
                }
            })
        });
        Ok(())
    }

    pub(crate) fn open_source(&self, node: &Node, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(url) = node.url.clone() {
            if url.starts_with(reference::SCHEME) {
                self.defer_window(window, cx, move |root, window, cx| {
                    root.open_reference(&url, window, cx)
                });
            } else {
                cx.open_url(&url);
            }
        } else if let Some(file) = &node.file {
            external::launch(self.path.join(file), cx);
        }
    }

    pub(crate) fn convert(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = references::current(cx) else {
            return;
        };
        let Some(project) = workspace.read(cx).project(self.canvas.project) else {
            return;
        };
        let name = reference::project_name(&project.path)
            .unwrap_or_default()
            .into_owned();
        let made = self
            .source
            .update(cx, |source, cx| source.convert_to_article(id, &name, cx));
        match made {
            Ok(_) => workspace.update(cx, |workspace, cx| {
                workspace.reload_project(self.canvas.project, cx)
            }),
            Err(error) => self.defer_window(window, cx, move |root, window, cx| {
                root.show_error("Cannot convert card".into(), error, window, cx)
            }),
        }
    }
}
