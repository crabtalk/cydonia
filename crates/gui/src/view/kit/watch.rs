//! The entities a view draws from, observed. Notifying an entity redraws only
//! the views observing it: a cached pane drawing it without a watch keeps
//! drawing what it held before.

use bezel::gpui::{Context, Entity, EntityId, FocusHandle, Subscription, Window};

#[derive(Default)]
pub(crate) struct Watch(Vec<(EntityId, Subscription)>);

impl Watch {
    /// Observe `entities` from the view `cx` is, redrawing it when one
    /// changes, and stop observing whatever else was observed before.
    pub(crate) fn set<'a, V: 'static, T: 'static>(
        &mut self,
        entities: impl IntoIterator<Item = &'a Entity<T>>,
        cx: &mut Context<V>,
    ) {
        self.reconcile(entities, |entity| {
            cx.observe(entity, |_, _, cx| cx.notify())
        });
    }

    /// [`Self::set`], running `on` when one changes instead of redrawing.
    pub(crate) fn set_in<'a, V: 'static, T: 'static>(
        &mut self,
        entities: impl IntoIterator<Item = &'a Entity<T>>,
        window: &mut Window,
        cx: &mut Context<V>,
        on: impl Fn(&mut V, Entity<T>, &mut Window, &mut Context<V>) + Clone + 'static,
    ) {
        self.reconcile(entities, |entity| {
            let on = on.clone();
            cx.observe_in(entity, window, move |this, entity, window, cx| {
                on(this, entity, window, cx)
            })
        });
    }

    fn reconcile<'a, T: 'static>(
        &mut self,
        entities: impl IntoIterator<Item = &'a Entity<T>>,
        mut observe: impl FnMut(&Entity<T>) -> Subscription,
    ) {
        let entities: Vec<&Entity<T>> = entities.into_iter().collect();
        let same = self.0.len() == entities.len()
            && self
                .0
                .iter()
                .zip(&entities)
                .all(|((id, _), entity)| *id == entity.entity_id());
        if same {
            return;
        }
        let mut held = std::mem::take(&mut self.0);
        self.0 = entities
            .into_iter()
            .map(|entity| {
                let id = entity.entity_id();
                match held.iter().position(|(at, _)| *at == id) {
                    Some(ix) => held.swap_remove(ix),
                    None => (id, observe(entity)),
                }
            })
            .collect();
    }
}

/// A focus handle a view draws from, observed: the view draws again when the
/// focus comes into the handle or anything inside it, and when it leaves.
pub(crate) struct FocusWatch {
    _subscriptions: [Subscription; 2],
}

impl FocusWatch {
    pub(crate) fn new<V: 'static>(
        focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        Self {
            _subscriptions: [
                cx.on_focus_in(focus, window, |_, _, cx| cx.notify()),
                cx.on_focus_out(focus, window, |_, _, _, cx| cx.notify()),
            ],
        }
    }
}
