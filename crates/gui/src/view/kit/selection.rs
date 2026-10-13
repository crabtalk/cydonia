//! One text selection across a set of read-only texts, each drawn with
//! [`markdown::selectable`] under a key of its own: which text holds the
//! selection, what of it, and the press that is extending it.

use bezel::{
    gpui::{App, FocusHandle},
    ui::input::{Granularity, drag_selection},
};
use markdown::{BlockLayouts, Cursor, Doc, Selection, selectable::Pointer};
use std::{cell::RefCell, collections::HashMap, hash::Hash, ops::Range, rc::Rc};

/// The selection over the texts keyed by `K`. One at a time: a press in
/// another text is what clears the last.
pub struct Selections<K> {
    held: Option<(K, Selection)>,
    /// The unit the press selected by and the span it took, while the button
    /// is down. A move only extends a selection a press started.
    pressed: Option<(Granularity, Range<Cursor>)>,
    /// The text the press came down in.
    pressed_doc: Option<Rc<Doc>>,
    /// What each text painted, so a press resolves against what is on screen.
    /// Behind a cell: the texts are drawn from shared borrows, and a text drawn
    /// for the first time has to be able to put its own in.
    layouts: RefCell<HashMap<K, BlockLayouts>>,
    /// Each text's focus, which is what copy is bound through.
    focus: RefCell<HashMap<K, FocusHandle>>,
}

impl<K> Default for Selections<K> {
    fn default() -> Self {
        Self {
            held: None,
            pressed: None,
            pressed_doc: None,
            layouts: RefCell::default(),
            focus: RefCell::default(),
        }
    }
}

impl<K: Copy + Eq + Hash> Selections<K> {
    /// The layout store for `key`, made on the first frame it is drawn.
    pub fn layouts(&self, key: K) -> BlockLayouts {
        self.layouts.borrow_mut().entry(key).or_default().clone()
    }

    /// How many texts have a layout store.
    pub fn laid_out(&self) -> usize {
        self.layouts.borrow().len()
    }

    /// Whether `key` has a layout store.
    pub fn is_laid_out(&self, key: K) -> bool {
        self.layouts.borrow().contains_key(&key)
    }

    /// The focus of `key`, made on the first frame it is drawn.
    pub fn focus(&self, key: K, cx: &mut App) -> FocusHandle {
        self.focus
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The focus of `key`, where it has been drawn.
    pub fn focus_of(&self, key: K) -> Option<FocusHandle> {
        self.focus.borrow().get(&key).cloned()
    }

    /// The text holding the selection, and what of it.
    pub fn held(&self) -> Option<(K, Selection)> {
        self.held
    }

    /// What `key` has selected, if it is the text holding the selection.
    pub fn of(&self, key: K) -> Option<Selection> {
        self.held
            .filter(|(held, _)| *held == key)
            .map(|(_, selection)| selection)
    }

    /// Whether the button is down on a press that started a selection.
    pub fn pressing(&self) -> bool {
        self.pressed.is_some()
    }

    /// The text the press came down in, while the button is down.
    pub fn pressed_doc(&self) -> Option<Rc<Doc>> {
        self.pressed_doc.clone()
    }

    /// Whether the pointer is dragging the head of `key`'s selection.
    pub fn dragging_in(&self, key: K) -> bool {
        self.pressed.is_some() && self.of(key).is_some()
    }

    /// Answer the pointer over `key`, whose text is `doc`. A press selects the
    /// unit under it there and drops whatever another text held; a move
    /// extends by that unit.
    pub fn point(&mut self, key: K, pointer: Pointer, doc: &Rc<Doc>) {
        match pointer {
            Pointer::Down(cursor, unit) => {
                let span = cursor.span(unit, doc);
                self.held = Some((key, Selection::new(span.start, span.end)));
                self.pressed = Some((unit, span));
                self.pressed_doc = Some(doc.clone());
            }
            Pointer::Move(cursor) => {
                let held = self.held.is_some_and(|(at, _)| at == key);
                if let Some((unit, pressed)) = self.pressed.clone().filter(|_| held) {
                    let (anchor, head) = drag_selection(pressed, cursor.span(unit, doc));
                    self.held = Some((key, Selection::new(anchor, head)));
                }
            }
            Pointer::Up => self.release(),
        }
    }

    /// The button came up: whatever the selection had become is what it is.
    pub fn release(&mut self) {
        self.pressed = None;
        self.pressed_doc = None;
    }

    /// Keep the layouts and focus of the texts `keep` names, and of the one
    /// holding the selection; drop the rest.
    pub fn retain(&self, keep: impl Fn(&K) -> bool) {
        let held = self.held.map(|(key, _)| key);
        let keep = |key: &K| keep(key) || held == Some(*key);
        self.layouts.borrow_mut().retain(|key, _| keep(key));
        self.focus.borrow_mut().retain(|key, _| keep(key));
    }

    /// Drop the selection.
    pub fn clear(&mut self) {
        self.held = None;
        self.release();
    }
}
