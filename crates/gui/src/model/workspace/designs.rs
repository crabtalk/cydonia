use crate::model::workspace::{Entry, Workspace};
use artifact::{design::Design, project::Project as _};
use bezel::gpui::{Context, Entity};

impl Workspace {
    pub fn design_entities(&self) -> Vec<Entity<Design>> {
        self.projects
            .iter()
            .flat_map(|project| &project.designs)
            .map(|held| held.design.clone())
            .collect()
    }

    pub fn design_entity(&self, entry: &Entry) -> Option<&Entity<Design>> {
        let project = self.project(entry.project)?;
        Some(&project.designs.get(project.design_ix(&entry.id)?)?.design)
    }

    pub fn edit_design(
        &mut self,
        entry: &Entry,
        edit: impl FnOnce(&mut Design),
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.project(entry.project) else {
            return false;
        };
        let Some(held) = self.design_entity(entry).cloned() else {
            return false;
        };
        let mut next = held.read(cx).clone();
        edit(&mut next);
        if project.store().save_design(&mut next).is_err() {
            return false;
        }
        let archive_changed = held.read(cx).archived != next.archived;
        held.update(cx, |held, cx| {
            *held = next;
            cx.notify();
        });
        if archive_changed {
            self.prune_archived(cx);
        }
        cx.notify();
        true
    }

    pub fn delete_design(&mut self, project: usize, ix: usize, cx: &mut Context<Self>) {
        let Some(project) = self.projects.get_mut(project) else {
            return;
        };
        let Some(held) = project.designs.get(ix) else {
            return;
        };
        if project.store().remove_design(&held.id).is_err() {
            return;
        }
        project.designs.remove(ix);
        project.design = project
            .design
            .filter(|at| *at != ix)
            .map(|at| if at > ix { at - 1 } else { at });
        cx.notify();
    }
}
