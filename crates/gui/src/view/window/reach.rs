//! How a view inside the window acts on it. The window may reach back into
//! the view that asked, and an entity already being updated cannot be updated
//! again, so the window's part runs once the view's own update has returned.

use crate::view::{kit::menu::Act, window::root::Cydonia};
use bezel::{
    gpui::{Context, WeakEntity, Window},
    ui::menu::Item,
};
use std::rc::Rc;

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
