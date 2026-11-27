use crate::model::{
    project::HeldDesign,
    store::Store,
    workspace::{Entry, Member, Workspace},
};
use artifact::{
    design::{Artboard, Design},
    error::Error,
    kind::Kind,
    project::Project as _,
};
use bezel::gpui::{Context, Entity};

impl Workspace {
    pub fn new_design(&mut self, project: usize, cx: &mut Context<Self>) -> Result<Member, String> {
        if !self.settings.features.designs {
            return Err("Designs are switched off.".into());
        }
        let open = self
            .projects
            .get_mut(project)
            .ok_or("project is no longer open")?;
        let design = open
            .store()
            .create_design("")
            .map_err(|error| error.to_string())?;
        let member = Member::new(open.id, Kind::Design, design.id.clone());
        open.designs.insert(0, HeldDesign::new(design, cx));
        cx.notify();
        Ok(member)
    }

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

    pub(super) fn shown_designs(&self) -> Vec<Entry> {
        self.arranged()
            .map(|space| space.tree.fronts())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|member| {
                member
                    .entry()
                    .filter(|entry| entry.kind == Kind::Design)
                    .cloned()
            })
            .collect()
    }

    pub fn load_design(&self, entry: &Entry, cx: &mut Context<Self>) {
        let Some(held) = self.design_entity(entry) else {
            return;
        };
        if held.read(cx).revisions_loaded {
            return;
        }
        if let Some(project) = self.project(entry.project) {
            let store = project.store();
            held.update(cx, |design, cx| {
                store.load_design_revisions(design);
                cx.notify();
            });
        }
    }

    pub fn edit_design<T>(
        &mut self,
        entry: &Entry,
        edit: impl FnOnce(&mut Design) -> Result<T, Error>,
        cx: &mut Context<Self>,
    ) -> Result<T, Error> {
        self.change_design(
            entry,
            |store, design| {
                let value = edit(design)?;
                store.save_design(design)?;
                Ok(value)
            },
            cx,
        )
    }

    pub fn add_design_artboard(
        &mut self,
        entry: &Entry,
        title: &str,
        cx: &mut Context<Self>,
    ) -> Result<Artboard, Error> {
        self.change_design(
            entry,
            |store, design| store.add_design_artboard(design, title),
            cx,
        )
    }

    pub fn remove_design_artboard(
        &mut self,
        entry: &Entry,
        artboard: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        self.change_design(
            entry,
            |store, design| store.remove_design_artboard(design, artboard),
            cx,
        )
    }

    /// Run `change` over a copy of the design, which saves it, and hold what
    /// it saved.
    fn change_design<T>(
        &mut self,
        entry: &Entry,
        change: impl FnOnce(&Store, &mut Design) -> Result<T, Error>,
        cx: &mut Context<Self>,
    ) -> Result<T, Error> {
        let gone = || Error::Gone {
            named: "This design".into(),
        };
        let project = self.project(entry.project).ok_or_else(gone)?;
        let held = self.design_entity(entry).cloned().ok_or_else(gone)?;
        let mut next = held.read(cx).clone();
        let value = change(&project.store(), &mut next)?;
        let archive_changed = held.read(cx).archived != next.archived;
        held.update(cx, |held, cx| {
            *held = next;
            cx.notify();
        });
        if archive_changed {
            self.prune_archived(cx);
        }
        cx.notify();
        Ok(value)
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
