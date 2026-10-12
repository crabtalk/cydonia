//! The margin beside an article: each comment's note level with the line it
//! is on, stacked so none overlaps the one above, or a mark per commented
//! line where the room is too narrow for notes.
//!
//! Notes are built and measured at prepaint, the way the document's column
//! builds its blocks, because how tall each note is decides where every note
//! under it goes.

use bezel::{
    gpui::{
        AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
        InspectorElementId, IntoElement, LayoutId, Pixels, Size, Style, Window, point, px,
        relative,
    },
    motion::{self, LAYOUT},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use web_time::Instant;

/// The widest a note is set.
pub const NOTE_WIDTH: f32 = 260.;

/// Room left of a note, between it and the column.
pub const NOTE_INSET: f32 = 16.;

/// Room a margin of marks takes.
pub const MARK_GUTTER: f32 = 40.;

/// Space between two stacked notes.
const GAP: f32 = 4.;

/// Tops closer than this are one line, and share a mark.
const SAME_LINE: f32 = 4.;

/// What the margin asks its host to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// One comment's note, sitting in the margin.
    Note(u64),
    /// One comment's note, floated over the page from a mark.
    Open(u64),
    /// The mark for a line, its first comment and how many it carries.
    Mark { first: u64, count: usize },
}

pub type Build = Rc<dyn Fn(Slot, &mut Window, &mut App) -> AnyElement>;

/// How far each note is pushed off its line, gliding from where it was to
/// where the stack wants it. Its line itself is never glided: a note moves
/// with its text at once.
#[derive(Default)]
pub struct Glide {
    /// By note: the push it is gliding from, the one it is gliding to, and
    /// since when.
    pushes: HashMap<u64, (Pixels, Pixels, Instant)>,
}

impl Glide {
    /// Where `id`, pushed `to` off its line, shows this frame, and whether it
    /// is still on its way.
    fn push(&mut self, id: u64, to: Pixels, reduced: bool, now: Instant) -> (Pixels, bool) {
        let total = LAYOUT.total().mul_f32(motion::speed_scale()).as_secs_f32();
        let at = |(from, to, since): (Pixels, Pixels, Instant)| {
            let raw = (now - since).as_secs_f32() / total.max(f32::EPSILON);
            let t = LAYOUT.progress(raw);
            (
                px(motion::lerp(f32::from(from), f32::from(to), t)),
                raw < 1.0,
            )
        };
        let entry = self.pushes.entry(id).or_insert((to, to, now));
        if reduced {
            *entry = (to, to, now);
        } else if (entry.1 - to).abs() > px(0.5) {
            *entry = (at(*entry).0, to, now);
        }
        at(*entry)
    }

    /// Forget notes no longer shown.
    fn keep(&mut self, shown: &[u64]) {
        self.pushes.retain(|id, _| shown.contains(id));
    }
}

pub struct Margin {
    /// Each comment and the top of its line from the margin's own top, in
    /// document order.
    notes: Vec<(u64, Pixels)>,
    /// The comment whose note sits exactly at its line.
    pinned: Option<u64>,
    /// How far above its line the open note starts.
    lift: Pixels,
    /// Notes where they fit, marks where they do not.
    notes_wanted: bool,
    build: Build,
    glide: Rc<RefCell<Glide>>,
    reduced: bool,
}

impl Margin {
    /// `glide` is held by whatever outlives a frame, and `reduced` snaps every
    /// note to where it goes.
    pub fn new(
        notes: Vec<(u64, Pixels)>,
        pinned: Option<u64>,
        lift: Pixels,
        notes_wanted: bool,
        build: Build,
        glide: Rc<RefCell<Glide>>,
        reduced: bool,
    ) -> Self {
        Self {
            notes,
            pinned,
            lift,
            notes_wanted,
            build,
            glide,
            reduced,
        }
    }

    /// Glide each of `keys`, wanting `wants`, to `tops`, and answer where each
    /// shows this frame.
    fn glided(
        &self,
        keys: &[u64],
        wants: &[Pixels],
        tops: Vec<Pixels>,
        window: &mut Window,
    ) -> Vec<Pixels> {
        let mut glide = self.glide.borrow_mut();
        glide.keep(keys);
        let now = Instant::now();
        let mut moving = false;
        let shown = keys
            .iter()
            .zip(wants.iter().zip(tops))
            .map(|(key, (want, top))| {
                let (push, on) = glide.push(*key, top - *want, self.reduced, now);
                moving |= on;
                *want + push
            })
            .collect();
        if moving {
            window.request_animation_frame();
        }
        shown
    }

    /// Whether the margin, `width` wide, sets notes rather than marks.
    pub fn fits_notes(width: Pixels) -> bool {
        width >= px(NOTE_INSET + NOTE_WIDTH * 0.75)
    }
}

/// Where each of `heights`, wanting `wants`, goes: none above the one before
/// it plus the gap, and `pinned` exactly where it wants, the rest stacked up
/// and down from it.
pub fn stack(wants: &[Pixels], heights: &[Pixels], pinned: Option<usize>) -> Vec<Pixels> {
    let gap = px(GAP);
    let mut tops = wants.to_vec();
    let from = pinned.unwrap_or(0);
    for ix in from + 1..tops.len() {
        tops[ix] = tops[ix].max(tops[ix - 1] + heights[ix - 1] + gap);
    }
    if pinned.is_some() {
        for ix in (0..from).rev() {
            tops[ix] = tops[ix].min(tops[ix + 1] - heights[ix] - gap);
        }
    }
    tops
}

impl IntoElement for Margin {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Margin {
    type RequestLayoutState = ();
    type PrepaintState = Vec<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let measure =
            |element: &mut AnyElement, width: Pixels, window: &mut Window, cx: &mut App| {
                let space = Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::MinContent,
                };
                element.layout_as_root(space, window, cx).height
            };
        let mut built = Vec::new();
        if self.notes_wanted && Margin::fits_notes(bounds.size.width) {
            let width = (bounds.size.width - px(NOTE_INSET)).min(px(NOTE_WIDTH));
            let mut wants = Vec::new();
            let mut heights = Vec::new();
            for (id, top) in &self.notes {
                let mut element = (self.build)(Slot::Note(*id), window, cx);
                heights.push(measure(&mut element, width, window, cx));
                wants.push(match self.pinned == Some(*id) {
                    true => *top - self.lift,
                    false => *top,
                });
                built.push(element);
            }
            let pinned = self
                .pinned
                .and_then(|pinned| self.notes.iter().position(|(id, _)| *id == pinned));
            let keys: Vec<u64> = self.notes.iter().map(|(id, _)| *id).collect();
            let tops = self.glided(&keys, &wants, stack(&wants, &heights, pinned), window);
            for (element, top) in built.iter_mut().zip(tops) {
                element.prepaint_at(bounds.origin + point(px(NOTE_INSET), top), window, cx);
            }
            return built;
        }

        let mut lines: Vec<(u64, usize, Pixels)> = Vec::new();
        for (id, top) in &self.notes {
            match lines.last_mut() {
                Some((_, count, at)) if (*top - *at).abs() < px(SAME_LINE) => *count += 1,
                _ => lines.push((*id, 1, *top)),
            }
        }
        let width = bounds.size.width;
        let mut wants = Vec::new();
        let mut heights = Vec::new();
        for (first, count, top) in &lines {
            let mut element = (self.build)(
                Slot::Mark {
                    first: *first,
                    count: *count,
                },
                window,
                cx,
            );
            heights.push(measure(&mut element, width, window, cx));
            wants.push(*top);
            built.push(element);
        }
        let keys: Vec<u64> = lines.iter().map(|(first, ..)| *first).collect();
        let tops = self.glided(&keys, &wants, stack(&wants, &heights, None), window);
        for (element, top) in built.iter_mut().zip(tops) {
            element.prepaint_at(bounds.origin + point(px(0.), top), window, cx);
        }
        // Over the page, its right edge at the marks.
        if let Some((id, top)) = self
            .pinned
            .and_then(|pinned| self.notes.iter().find(|(id, _)| *id == pinned))
        {
            let mut element = (self.build)(Slot::Open(*id), window, cx);
            measure(&mut element, px(NOTE_WIDTH), window, cx);
            let x = bounds.size.width - px(MARK_GUTTER + NOTE_WIDTH);
            element.prepaint_at(bounds.origin + point(x, *top - self.lift), window, cx);
            built.push(element);
        }
        built
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        built: &mut Vec<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        for element in built {
            element.paint(window, cx);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/margin.rs"]
mod tests;
