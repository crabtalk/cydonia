//! The one entity a view draws from, observed. Notifying an entity redraws
//! only the views observing it: a cached pane drawing it without a watch keeps
//! drawing what it held before.

use bezel::gpui::{Context, Entity, EntityId, Subscription};

#[derive(Default)]
pub(crate) struct Watch(Option<(EntityId, Subscription)>);

impl Watch {
    /// Observe `entity` from the view `cx` is, dropping whatever was observed
    /// before. A no-op while it is already the one observed.
    pub(crate) fn set<V: 'static, T: 'static>(
        &mut self,
        entity: Option<&Entity<T>>,
        cx: &mut Context<V>,
    ) {
        let id = entity.map(Entity::entity_id);
        if self.0.as_ref().map(|(id, _)| *id) == id {
            return;
        }
        self.0 = entity.map(|entity| {
            let watch = cx.observe(entity, |_, _, cx| cx.notify());
            (entity.entity_id(), watch)
        });
    }
}
