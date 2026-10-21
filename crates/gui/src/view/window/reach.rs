//! How a view inside the window acts on it. The window may reach back into
//! the view that asked, and an entity already being updated cannot be updated
//! again, so the window's part runs once the view's own update has returned.

use crate::view::{
    kit::menu::{self, Act},
    window::root::Cydonia,
};
use bezel::{
    gpui::{Context, WeakEntity, Window},
    ui::{
        context_menu::{ContextMenu, Place, Slot},
        menu::Item,
    },
};
use std::{cell::RefCell, rc::Rc};

pub(crate) trait InWindow: Sized + 'static {
    fn root(&self) -> &WeakEntity<Cydonia>;

    /// Run `act` on the window once this view is no longer being updated.
    fn defer_window(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        act: impl FnOnce(&mut Cydonia, &mut Window, &mut Context<Cydonia>) + 'static,
    ) {
        let root = self.root().clone();
        window.defer(cx, move |window, cx| {
            root.update(cx, |root, cx| act(root, window, cx)).ok();
        });
    }

    /// Open this view's menu `key` at `place` with the rows the window builds
    /// for it, or shut it where it is the one up.
    fn window_menu<K: PartialEq + 'static>(
        &self,
        slot: Slot<Self, K>,
        key: K,
        place: Place,
        rows: impl FnOnce(&mut Cydonia, &mut Window, &mut Context<Cydonia>) -> Vec<(Item, Act<Cydonia>)>
        + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        self.defer_window(window, cx, move |root, window, cx| {
            let rows = RefCell::new(Some(Self::through(rows(root, window, cx))));
            view.update(cx, |this, cx| {
                ContextMenu::toggle(
                    this,
                    slot,
                    key,
                    place,
                    move |_, _, _| menu::chooser(rows.borrow_mut().take().unwrap_or_default()),
                    window,
                    cx,
                )
            });
        });
    }

    /// Rows the window builds, picked from this view's menu: each runs on the
    /// window once the view's own handling is done.
    fn through(rows: Vec<(Item, Act<Cydonia>)>) -> Vec<(Item, Act<Self>)> {
        rows.into_iter()
            .map(|(item, act)| {
                let act = Rc::new(act);
                let act: Act<Self> = Box::new(move |this, path, window, cx| {
                    let (act, path) = (act.clone(), path.to_vec());
                    this.defer_window(window, cx, move |root, window, cx| {
                        act(root, &path, window, cx)
                    });
                });
                (item, act)
            })
            .collect()
    }
}
