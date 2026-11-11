use crate::{
    model::{
        media::Png as _,
        project::ProjectId,
        workspace::{Member, Opening},
    },
    view::{
        canvas::CanvasView,
        window::{
            reach::InWindow,
            root::{Cydonia, ImportCanvas},
        },
    },
};
use artifact::{canvas::bundle::Bundle, kind::Kind};
use bezel::gpui::{Context, PathPromptOptions, Window};
use image::DynamicImage;

impl Cydonia {
    pub(crate) fn import_canvas_action(
        &mut self,
        _: &ImportCanvas,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self
            .workspace
            .read(cx)
            .active_project()
            .map(|project| project.id)
        {
            self.import_canvas(project, window, cx);
        }
    }

    pub(crate) fn import_canvas(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.read(cx);
        if !workspace.settings.features.canvases {
            return;
        }
        let Some(open) = workspace.project(project) else {
            return;
        };
        let store = open.store();
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import canvas".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { store.import_canvas(&path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(record) => {
                    this.workspace
                        .update(cx, |workspace, cx| workspace.reload_project(project, cx));
                    this.enter_member(
                        Member::new(project, Kind::Canvas, record.id),
                        Opening::Kept,
                        window,
                        cx,
                    );
                }
                Err(error) => {
                    this.show_error("Cannot import canvas".into(), error.to_string(), window, cx)
                }
            });
        })
        .detach();
    }
}

impl CanvasView {
    pub(crate) fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.source.read(cx);
        let document = (**source.document.read(cx)).clone();
        let name = format!(
            "{}.canvas",
            source.record.label().replace(['/', '\\', ':'], "-")
        );
        let root = self.path.clone();
        let picked = cx.prompt_for_new_path(&root, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { Bundle::collect(document, &root)?.export(&path) })
                .await;
            if let Err(error) = result {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.show_error(
                            "Cannot export canvas".into(),
                            error.to_string(),
                            window,
                            cx,
                        );
                    });
                });
            }
        })
        .detach();
    }
}

impl CanvasView {
    pub(crate) fn export_image(
        &mut self,
        area: canvas::export::Area,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.exporting_image {
            return;
        }
        let name = format!(
            "{}.png",
            self.source
                .read(cx)
                .record
                .label()
                .replace(['/', '\\', ':'], "-")
        );
        let picked = cx.prompt_for_new_path(&self.path, Some(&name));
        self.exporting_image = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                let _ = this.update(cx, |this, cx| {
                    this.exporting_image = false;
                    cx.notify();
                });
                return;
            };
            let rendered = this.update(cx, |this, cx| {
                this.surface
                    .update(cx, |surface, cx| surface.export_image(area, scale, cx))
            });
            let result = async {
                let task = rendered
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?
                    .map_err(anyhow::Error::msg)?;
                let image = task.await?;
                cx.background_executor()
                    .spawn(async move {
                        let bytes = DynamicImage::ImageRgba8(image).png()?;
                        artifact::fs::save(&path, bytes)?;
                        anyhow::Ok(())
                    })
                    .await
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.exporting_image = false;
                cx.notify();
                if let Err(error) = result {
                    this.defer_window(window, cx, move |root, window, cx| {
                        root.show_error("Cannot export image".into(), error.to_string(), window, cx)
                    });
                }
            });
        })
        .detach();
    }
}
