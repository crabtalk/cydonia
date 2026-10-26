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
