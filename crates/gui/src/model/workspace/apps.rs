//! The apps on this machine. An app belongs to no project: the workspace holds
//! every one, and developer mode decides which of them are listed and opened.
//!
//! A continuation of [`Workspace`]'s one `impl`, which is why it opens on
//! `use super::*`: these methods work on the same struct and reach the same
//! names as the rest of it.
use super::*;
use crate::model::apps::{HeldApp, Origin, Program};

impl Workspace {
    /// Re-list the apps, keeping the program of every app still there.
    pub fn reload_apps(&mut self, cx: &mut Context<Self>) {
        let mut held: HashMap<String, HeldApp> = self
            .apps
            .drain(..)
            .map(|held| (held.id.clone(), held))
            .collect();
        self.apps = Origin::list()
            .into_iter()
            .map(|origin| match held.remove(origin.id()) {
                Some(kept) => {
                    kept.program
                        .update(cx, |program, cx| program.adopt(origin, cx));
                    kept
                }
                None => HeldApp::new(origin, cx),
            })
            .collect();
        if !held.is_empty() {
            self.prune_spaces(cx);
        }
        cx.notify();
    }

    /// The apps listed and opened: every one, or with developer mode off,
    /// the installed ones.
    pub fn listed_apps(&self, cx: &App) -> impl Iterator<Item = &HeldApp> {
        let developer = self.settings.developer_mode;
        self.apps
            .iter()
            .filter(move |held| developer || !held.program.read(cx).local())
    }

    /// The program of the app `id`, where it is listed.
    pub fn program(&self, id: &str, cx: &App) -> Option<&bezel::gpui::Entity<Program>> {
        self.listed_apps(cx)
            .find(|held| held.id == id)
            .map(|held| &held.program)
    }

    /// Every app's program, which what lists apps observes.
    pub fn programs(&self) -> impl Iterator<Item = &bezel::gpui::Entity<Program>> {
        self.apps.iter().map(|held| &held.program)
    }

    /// Show the app `id` in the pane a window with no space open shows.
    pub fn open_app(&mut self, id: &str, cx: &mut Context<Self>) {
        self.app = Some(id.to_owned());
        cx.notify();
    }

    /// An app was recorded: watch the apps again, so a folder loaded is
    /// among them, and re-list them.
    pub fn app_recorded(&mut self, cx: &mut Context<Self>) {
        self.apps_watch = Some(Watch::apps(cx));
        self.reload_apps(cx);
    }

    /// The apps named moved: re-list them, and build again each of the named
    /// ones a view runs.
    pub fn apps_changed(&mut self, ids: &[String], cx: &mut Context<Self>) {
        self.reload_apps(cx);
        for held in &self.apps {
            if ids.contains(&held.id) {
                held.program.update(cx, |program, cx| program.changed(cx));
            }
        }
    }

    /// Forget the app `id`: its build stops, and its record and its own
    /// things go.
    pub fn delete_app(&mut self, id: &str, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let Some(held) = self.apps.iter().find(|held| held.id == id) else {
            return Ok(());
        };
        held.program.update(cx, |program, _| program.stop());
        held.program.read(cx).origin.remove()?;
        if self.app.as_deref() == Some(id) {
            self.app = None;
        }
        self.reload_apps(cx);
        Ok(())
    }

    /// Switch developer mode. Loaded apps stay held either way: off, they are
    /// not listed or opened.
    pub fn set_developer_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        if settings::set_developer_mode(on).is_err() {
            return;
        }
        self.settings.developer_mode = on;
        cx.notify();
    }
}
