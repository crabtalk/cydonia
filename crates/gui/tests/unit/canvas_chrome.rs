use crate::view::canvas::chrome::Chrome;
use bezel::gpui::{AppContext as _, TestAppContext, WeakEntity};
use canvas::{Canvas, CanvasView, layout};
use std::{cell::Cell, rc::Rc};

#[gpui::test]
fn toolbar_observes_read_only_changes_without_camera_notifications(cx: &mut TestAppContext) {
    let surface = cx.new(|cx| CanvasView::new(Canvas::default(), layout::FREE, cx));
    let chrome = cx.new(|cx| Chrome::new(WeakEntity::new_invalid(), &surface, cx));
    let notifications = Rc::new(Cell::new(0));
    let _watch = cx.update(|cx| {
        cx.observe(&chrome, {
            let notifications = notifications.clone();
            move |_, _| notifications.set(notifications.get() + 1)
        })
    });
    surface.update(cx, |surface, cx| {
        surface.update_editor(cx, |editor| editor.pan_by(10., 20.))
    });
    assert_eq!(notifications.get(), 0);
    surface.update(cx, |surface, cx| {
        surface.update_editor(cx, |editor| {
            let mut options = editor.options();
            options.read_only = true;
            editor.set_options(options);
        })
    });
    assert_eq!(notifications.get(), 1);
}
