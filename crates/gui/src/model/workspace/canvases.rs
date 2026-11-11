use crate::model::{
    canvas::Canvas,
    project::HeldCanvas,
    workspace::{Entry, Member, Workspace},
};
use artifact::{kind::Kind, project::Project as _};
use bezel::gpui::{Context, Entity};

impl Workspace {
    pub fn canvas_entities(&self) -> Vec<Entity<Canvas>> {
        self.projects
            .iter()
            .flat_map(|project| &project.canvases)
            .map(|held| held.canvas.clone())
            .collect()
    }

    pub fn canvas_entity(&self, entry: &Entry) -> Option<&Entity<Canvas>> {
        let open = self.project(entry.project)?;
        Some(&open.canvases.get(open.canvas_ix(&entry.id)?)?.canvas)
    }

    pub fn create_canvas(
        &mut self,
        project: usize,
        cx: &mut Context<Self>,
    ) -> Result<Member, String> {
        if !self.settings.features.canvases {
            return Err("Canvas is switched off.".to_owned());
        }
        let open = self
            .projects
            .get_mut(project)
            .ok_or("project is no longer open")?;
        let store = open.store();
        let record = store
            .create_canvas("", &canvas::Canvas::default())
            .map_err(|error| error.to_string())?;
        let member = Member::new(open.id, Kind::Canvas, record.id.clone());
        open.canvases.insert(0, HeldCanvas::new(record, store, cx));
        cx.notify();
        Ok(member)
    }

    pub fn rename_canvas(&mut self, entry: &Entry, title: String, cx: &mut Context<Self>) {
        if let Some(canvas) = self.canvas_entity(entry).cloned() {
            canvas.update(cx, |canvas, cx| {
                canvas.rename(title, cx);
            });
            cx.notify();
        }
    }

    pub fn archive_canvas(
        &mut self,
        entry: &Entry,
        archived: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(canvas) = self.canvas_entity(entry).cloned() else {
            return false;
        };
        if !canvas.update(cx, |canvas, cx| canvas.archive(archived, cx)) {
            return false;
        }
        self.prune_archived(cx);
        cx.notify();
        true
    }

    pub fn delete_canvas(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(open) = self.projects.get_mut(project) else {
            return;
        };
        let Some(held) = open.canvases.get(ix) else {
            return;
        };
        if open.store().remove_canvas(&held.id).is_err() {
            return;
        }
        open.canvases.remove(ix);
        open.canvas = open
            .canvas
            .filter(|at| *at != ix)
            .map(|at| if at > ix { at - 1 } else { at });
        cx.notify();
    }
}
