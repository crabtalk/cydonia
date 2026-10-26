//! The agent identities the workspace holds, and the sessions started as one.

use super::*;
use crate::model::identity::Store as _;

impl Workspace {
    /// The identity with this id. Nothing for an id no identity holds: an
    /// orphaned session's.
    pub fn identity(&self, id: &str) -> Option<&Identity> {
        self.identities.iter().find(|identity| identity.id == id)
    }

    /// Start a session as the identity with this id, in front of its
    /// workspace's project, opening the project where it is shut. Nothing
    /// where the identity is gone, its program is not installed, or sessions
    /// are off.
    pub fn new_session_as(
        &mut self,
        identity: &str,
        seed: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let identity = self.identity(identity)?.clone();
        let entry = named(&self.settings.agents, Some(&identity.acp), &identity.acp)?.clone();
        self.open_project(identity.workspace.clone(), cx);
        let project = self.ids.intern(&identity.workspace);
        let ix = self.project_ix(project)?;
        self.start_session(ix, true, cx, |id, _, cx| {
            ChatSession::connect_as(id, entry, &identity, seed, cx)
        })
    }

    /// Keep a new identity. Answers its id; nothing where it could not be
    /// written.
    pub fn create_identity(
        &mut self,
        identity: Identity,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let identity = identity::Shelf::create(identity)?;
        let id = identity.id.clone();
        self.identities.push(identity);
        cx.notify();
        Some(id)
    }

    /// Write `identity` over the one with its id, under the handle that one
    /// has.
    pub fn update_identity(&mut self, identity: Identity, cx: &mut Context<Self>) {
        let id = identity.id.clone();
        self.edit_identity(
            &id,
            |held| {
                *held = Identity {
                    handle: std::mem::take(&mut held.handle),
                    ..identity
                }
            },
            cx,
        );
        cx.notify();
    }

    /// Remove the identity with this id, its directory with it. Its sessions
    /// stay in their projects, orphaned.
    pub fn remove_identity(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(at) = self
            .identities
            .iter()
            .position(|identity| identity.id == id)
        else {
            return;
        };
        let identity = self.identities.remove(at);
        identity::Shelf::remove(&identity.handle);
        cx.notify();
    }

    /// Change the identity with this id and write it back. Every session held
    /// as it takes its choices from then on.
    pub(super) fn edit_identity(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Identity),
        cx: &mut Context<Self>,
    ) {
        let Some(identity) = self
            .identities
            .iter_mut()
            .find(|identity| identity.id == id)
        else {
            return;
        };
        change(identity);
        identity::Shelf::save(identity);
        let identity = identity.clone();
        for project in &self.projects {
            for held in &project.sessions {
                held.chat.update(cx, |chat, cx| {
                    if chat.identity.as_deref() == Some(identity.id.as_str()) {
                        chat.adopt(&identity);
                        cx.notify();
                    }
                });
            }
        }
    }
}
