//! How the workspace search finds entries off disk and narrows the sidebar.

use super::*;
use crate::view::window::root::Cydonia;
use bezel::gpui::{Context, Entity, IntoElement, Window, div, prelude::*, px};
use bezel::theme::Theme;

#[test]
fn the_title_outranks_the_body_and_earlier_blocks_outrank_later() {
    assert!(rank(&Block::Title) < rank(&Block::Line(0)));
    assert!(rank(&Block::Chat(1)) < rank(&Block::Chat(4)));
}

#[gpui::test]
fn workspace_search_keeps_results_beyond_the_palette_limit(cx: &mut gpui::TestAppContext) {
    use artifact::project::Project as _;
    let root = std::env::temp_dir().join(format!("cydonia-applied-search-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = fs::Project::new(&root);
    for ix in 0..crate::view::modals::palette::SHOWN + 1 {
        store
            .create_article(&format!("# Note {ix}\n\nretry request\n\nlater retry"))
            .unwrap();
    }
    let found = search_all(
        std::slice::from_ref(&root),
        &Query::literal("retry").unwrap(),
    );
    assert_eq!(found.len(), crate::view::modals::palette::SHOWN + 1);
    assert!(
        found
            .iter()
            .all(|hit| hit.kind == Kind::Article && !hit.in_title)
    );
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window = cx
        .add_window(|window, cx| Cydonia::new(Default::default(), Default::default(), window, cx));
    window
        .update(cx, |this, _, cx| {
            this.workspace.update(cx, |workspace, cx| {
                let mut project = crate::model::project::Project::new(root.clone(), false, cx);
                project.expanded = false;
                workspace.projects.push(project);
            });
            let before = this.list(cx).rows();
            this.search.update(cx, |search, _| {
                search.applied = Some(Applied {
                    query: Query::literal("retry").unwrap(),
                    filter: Some(Kind::Article),
                    found,
                    ready: true,
                })
            });
            this.push_finds(cx);
            assert_eq!(
                this.list(cx)
                    .rows()
                    .iter()
                    .filter(|row| matches!(
                        row,
                        Row::Entry {
                            showing: crate::model::workspace::Showing::Article(_),
                            ..
                        }
                    ))
                    .count(),
                crate::view::modals::palette::SHOWN + 1
            );
            assert!(!this.workspace.read(cx).projects[0].expanded);
            this.search.update(cx, Search::refresh);
            assert!(this.search.read(cx).applied.as_ref().unwrap().ready);
            assert_eq!(
                this.search.read(cx).rows(cx).unwrap().len(),
                crate::view::modals::palette::SHOWN + 1
            );
            this.workspace.update(cx, |workspace, cx| {
                workspace.active = Some(0);
                workspace.projects[0].article = Some(0);
                workspace.projects[0].articles[0].open(cx);
            });
            this.leaf()
                .clone()
                .update(cx, |pane, _| pane.kind = artifact::kind::Kind::Article);
            this.sync_items(cx);
            let editor = this.pane_editor(cx).unwrap();
            let matches = crate::view::pane::find::hits(
                editor.read(cx).doc(),
                this.search.read(cx).query().unwrap(),
            );
            assert_eq!(matches.len(), 2);
            editor.update(cx, |editor, cx| editor.select(matches[1], cx));
            this.leaf().clone().update(cx, |pane, _| pane.find_at = 1);
            this.reveal_applied_match(cx);
            assert_eq!(editor.read(cx).selection(), matches[0]);
            assert_eq!(this.leaf().read(cx).find_at, 0);
            this.search.update(cx, |search, _| {
                search.applied.as_mut().unwrap().filter = Some(Kind::Board)
            });
            assert!(this.list(cx).rows().is_empty());
            this.search.update(cx, Search::clear);
            this.push_finds(cx);
            editor.update(cx, |editor, cx| editor.select(matches[1], cx));
            this.reveal_applied_match(cx);
            assert_eq!(editor.read(cx).selection(), matches[1]);
            assert!(this.list(cx).rows() == before);
        })
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
}

struct SearchControls(Entity<Cydonia>);

impl gpui::Render for SearchControls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |root, cx| {
            div()
                .size_full()
                .flex()
                .child(
                    div()
                        .w(px(240.))
                        .h(px(300.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .child(root.sidebar.clone()),
                )
                .child(
                    div().relative().flex_1().h(px(200.)).children(
                        root.leaf()
                            .clone()
                            .update(cx, |pane, cx| pane.search_pill(cx)),
                    ),
                )
        })
    }
}

#[gpui::test]
fn applied_board_search_has_visible_working_exit_controls(cx: &mut gpui::TestAppContext) {
    use bezel::gpui::{Modifiers, MouseButton, VisualTestContext, size};
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Dark, cx));
    let window = cx.add_window(|window, cx| {
        let root = cx.new(|cx| Cydonia::new(Default::default(), Default::default(), window, cx));
        root.update(cx, |root, cx| {
            root.leaf()
                .clone()
                .update(cx, |pane, _| pane.kind = artifact::kind::Kind::Board)
        });
        cx.observe(&root, |_, _, cx| cx.notify()).detach();
        SearchControls(root)
    });
    let root = window
        .root(cx)
        .unwrap()
        .read_with(cx, |view, _| view.0.clone());
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(700.), px(300.)));
    for selector in ["clear-workspace-search", "find-close"] {
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                let query =
                    Query::literal("a very long query that must leave room for the close button");
                root.search
                    .update(cx, |search, cx| search.apply(query.unwrap(), None, cx));
                root.push_finds(cx);
                assert!(!root.leaf().read(cx).find(cx).text().is_empty());
            })
        });
        cx.run_until_parked();
        let sidebar_close = cx.debug_bounds("clear-workspace-search").unwrap();
        assert!(sidebar_close.right() <= px(240.));
        assert_eq!(sidebar_close.size.width, px(24.));
        let applied_bounds = cx.debug_bounds("search-pill").unwrap();
        assert_eq!(applied_bounds.size, size(px(240.), px(26.)));
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.leaf().clone().update(cx, |pane, cx| {
                    pane.finding = true;
                    pane.find_field
                        .update(cx, |field, cx| field.set_content("local query", cx));
                    cx.notify();
                });
            })
        });
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("search-pill").unwrap(), applied_bounds);
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.leaf().clone().update(cx, |pane, cx| {
                    pane.finding = false;
                    cx.notify();
                });
            })
        });
        cx.run_until_parked();
        let center = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(root.read(cx).search.read(cx).query().is_none());
            assert!(root.read(cx).leaf().read(cx).find(cx).text().is_empty());
        });
        assert!(cx.debug_bounds("search-pill").is_none());
    }
}
