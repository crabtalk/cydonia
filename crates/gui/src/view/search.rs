//! Searching every open project from the sidebar: the list narrows to the
//! entries whose text holds the query, and opening one puts the query in its
//! pane's find bar — see [`crate::view::find`].
//!
//! Each search reads the projects off disk on a background thread — see
//! [`artifact::search::disk`] — so an entry's hits are as of its last write.

use crate::view::root::Cydonia;
use artifact::{
    project::fs,
    search::{self, Kind, Query},
};
use bezel::{
    gpui::{
        self, AnyElement, App, Context, Entity, Focusable as _, KeyBinding, Task, Window, actions,
        div, prelude::*, px,
    },
    theme::Theme,
    ui::{
        icons,
        input::{FieldEvent, TextField},
        tooltip::Tooltip,
        widgets::Buttons as _,
    },
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
};

actions!(cydonia_search, [ToggleSearch, DismissSearch]);

const CONTEXT: &str = "CydoniaSearch";

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", DismissSearch, Some(CONTEXT))]
}

/// What matched, by project directory: each entry by its kind and id.
pub(crate) type Hits = HashMap<PathBuf, HashSet<(Kind, String)>>;

pub(crate) struct Search {
    pub(crate) open: bool,
    pub(crate) field: Entity<TextField>,
    /// The last finished search. `None` while nothing is searched for, which
    /// is when the sidebar lists everything.
    pub(crate) hits: Option<Rc<Hits>>,
    task: Option<Task<()>>,
}

impl Search {
    pub(crate) fn new(cx: &mut Context<Cydonia>) -> Self {
        let field = cx.new(|cx| {
            TextField::new(cx)
                .with_key_context(CONTEXT)
                .with_placeholder("search projects…")
        });
        cx.subscribe(&field, |this, _, event: &FieldEvent, cx| {
            if matches!(event, FieldEvent::Changed(_)) {
                this.refresh_search(cx);
            }
        })
        .detach();
        Self {
            open: false,
            field,
            hits: None,
            task: None,
        }
    }

    fn query(&self, cx: &App) -> Option<Query> {
        self.open
            .then(|| Query::literal(self.field.read(cx).content()))
            .flatten()
    }
}

impl Cydonia {
    pub(crate) fn toggle_search(
        &mut self,
        _: &ToggleSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.search.field.read(cx).focus_handle(cx);
        if self.search.open && focus.is_focused(window) {
            self.dismiss_search(&DismissSearch, window, cx);
            return;
        }
        self.search.open = true;
        self.sidebar_open = true;
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn dismiss_search(
        &mut self,
        _: &DismissSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.open = false;
        self.search.field.update(cx, |field, cx| field.clear(cx));
        window.focus(&self.leaf().focus, cx);
        cx.notify();
    }

    /// Search again for what the field holds. A search still running is
    /// dropped with its task.
    fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let Some(query) = self.search.query(cx) else {
            self.search.task = None;
            self.search.hits = None;
            cx.notify();
            return;
        };
        let roots: Vec<PathBuf> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .map(|open| open.path.clone())
            .collect();
        self.search.task = Some(cx.spawn(async move |this, cx| {
            let hits = cx
                .background_executor()
                .spawn(async move { search_all(&roots, &query) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.search.hits = Some(Rc::new(hits));
                cx.notify();
            });
        }));
    }

    /// Put the sidebar's query in the find bar of the pane now in front, so
    /// what matched is marked in what was opened.
    pub(crate) fn carry_search(&mut self, cx: &mut Context<Self>) {
        if self.search.hits.is_none() {
            return;
        }
        let query = self.search.field.read(cx).content().clone();
        let field = self.leaf().find_field.clone();
        self.leaf_mut().finding = true;
        field.update(cx, |field, cx| field.set_content(query, cx));
        cx.notify();
    }

    /// The field over the sidebar's list, while it is up.
    pub(crate) fn search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search.open {
            return None;
        }
        let theme = Theme::of(cx).clone();
        Some(
            div()
                .flex_none()
                .mx(px(8.))
                .mb(px(6.))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(2.))
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface_raised)
                .on_action(cx.listener(Self::dismiss_search))
                .child(
                    icons::icon(icons::text::Search)
                        .size(px(14.))
                        .text_color(theme.text_faint),
                )
                .child(div().flex_1().min_w_0().child(self.search.field.clone()))
                .child(
                    theme
                        .ghost("search-close")
                        .p(px(4.))
                        .rounded_full()
                        .child(
                            icons::icon(icons::notifications::X)
                                .size(px(12.))
                                .text_color(theme.text_faint),
                        )
                        .tooltip(|window, cx| Tooltip::text("Stop searching", window, cx))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_search(&DismissSearch, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

/// Every entry in `roots` holding `query`, by project.
fn search_all(roots: &[PathBuf], query: &Query) -> Hits {
    let kinds = [Kind::Article, Kind::Board, Kind::Session];
    roots
        .iter()
        .map(|root| {
            let (tx, rx) = mpsc::channel();
            search::disk(&fs::Project::new(root), &kinds, query, &tx);
            drop(tx);
            let found = rx
                .into_iter()
                .map(|hit| (hit.item.kind, hit.item.id))
                .collect();
            (root.clone(), found)
        })
        .collect()
}
