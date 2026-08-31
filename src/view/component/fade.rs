//! The fade that stands in for chrome painted over content.
//!
//! Over a see-through backdrop no overlay can fade content out: "what is
//! behind the window" is not a colour anything can paint, so every tint or
//! blur put in the header's place is a coat on the column rather than a view
//! through it. The content is what fades.

use bezel::gpui::{
    AnyElement, App, Bounds, EdgeFade, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, ScrollHandle, Window, px,
};
use std::panic::Location;

/// Fade `child` into the chrome standing over its top edge: everything within
/// `inset` of that edge is gone, and `band` below it is the ramp. gpui clamps
/// the ramp to zero past an active edge, which is what makes the inset a hard
/// cut rather than a steeper gradient — a line has to be invisible before it
/// reaches the title, not merely dim.
pub fn under(
    inset: f32,
    band: f32,
    scroll: Option<ScrollHandle>,
    child: impl IntoElement,
) -> Under {
    Under {
        inset,
        band,
        scroll,
        child: child.into_any_element(),
    }
}

pub struct Under {
    inset: f32,
    band: f32,
    scroll: Option<ScrollHandle>,
    child: AnyElement,
}

impl Element for Under {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // Read here, not at render: render rides the last frame's offset, and
        // on the frame a scroll is clamped away nothing re-renders — leaving a
        // fade on screen with nothing scrolled under it.
        let scrolled = self
            .scroll
            .as_ref()
            .is_some_and(|scroll| -f32::from(scroll.offset().y) > 1.0);
        let fade = scrolled.then(|| {
            let mut bounds = bounds;
            bounds.origin.y += px(self.inset);
            bounds.size.height -= px(self.inset);
            EdgeFade {
                bounds,
                band: px(self.band),
                top: true,
                bottom: false,
                left: false,
                right: false,
            }
        });
        window.with_edge_fade(fade, |window| self.child.paint(window, cx));
    }
}

impl IntoElement for Under {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
