//! The window's one modal: a view that holds the focus until it emits
//! [`DismissEvent`] or `escape` is pressed. The view draws its own scrim.

use crate::view::window::root::Cydonia;
use bezel::gpui::{
    self, AnyView, Context, DismissEvent, FocusHandle, KeyBinding, ManagedView, Subscription,
    Window, actions, div, prelude::*,
};

/// Claimed around the modal, so `escape` dismisses whichever is up.
const MODAL_CONTEXT: &str = "CydoniaModal";

actions!(cydonia_modal, [Dismiss]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", Dismiss, Some(MODAL_CONTEXT))]
}

struct Active {
    view: AnyView,
    /// Tracked around the view: holds the focus while anything in it does.
    focus: FocusHandle,
    /// What held the focus before, given it back on hide if the modal still
    /// holds it.
    previous: Option<FocusHandle>,
    _dismiss: Subscription,
}

#[derive(Default)]
pub(crate) struct ModalLayer {
    active: Option<Active>,
}

impl ModalLayer {
    /// Show the `V` that `build` makes, focused. A `V` already up is hidden
    /// instead; another modal is replaced.
    pub(crate) fn toggle<V: ManagedView>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) {
        let mut previous = window.focused(cx);
        if let Some(active) = &self.active {
            let same = active.view.clone().downcast::<V>().is_ok();
            previous = active.previous.clone();
            self.hide(window, cx);
            if same {
                return;
            }
        }
        let view = cx.new(|cx| build(window, cx));
        let dismiss = cx.subscribe_in(&view, window, |this, _, _: &DismissEvent, window, cx| {
            this.hide(window, cx);
        });
        let target = view.read(cx).focus_handle(cx);
        self.active = Some(Active {
            view: view.into(),
            focus: cx.focus_handle(),
            previous,
            _dismiss: dismiss,
        });
        cx.defer_in(window, move |_, window, cx| window.focus(&target, cx));
        cx.notify();
    }

    /// Take the modal down. Whether there was one.
    pub(crate) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.active.take() else {
            return false;
        };
        if let Some(previous) = active.previous
            && active.focus.contains_focused(window, cx)
        {
            window.focus(&previous, cx);
        }
        cx.notify();
        true
    }
}

impl Render for ModalLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = &self.active else {
            return div();
        };
        div()
            .absolute()
            .inset_0()
            .key_context(MODAL_CONTEXT)
            .track_focus(&active.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                this.hide(window, cx);
            }))
            .child(active.view.clone())
    }
}

impl Cydonia {
    /// [`ModalLayer::toggle`] on this window's layer.
    pub(crate) fn toggle_modal<V: ManagedView>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) {
        self.modals
            .update(cx, |modals, cx| modals.toggle(window, cx, build));
    }
}
