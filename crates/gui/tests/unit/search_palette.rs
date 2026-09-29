//! How a palette row cuts the line its match is on.

use super::*;

#[test]
fn a_short_line_is_kept_whole_with_the_match_where_it_was() {
    let (text, at) = clipped("  the pump runs", 6..10);
    assert_eq!(text.as_ref(), "the pump runs");
    assert_eq!(&text[at], "pump");
}

#[test]
fn a_long_line_is_cut_ahead_of_the_match_and_says_so() {
    let line = format!("{}needle and after", "word ".repeat(40));
    let start = line.find("needle").unwrap();
    let (text, at) = clipped(&line, start..start + 6);
    assert!(text.starts_with('…'), "{text}");
    assert_eq!(&text[at], "needle");
}

#[test]
fn the_title_outranks_the_body_and_earlier_blocks_outrank_later() {
    assert!(rank(&Block::Title) < rank(&Block::Line(0)));
    assert!(rank(&Block::Chat(1)) < rank(&Block::Chat(4)));
}

#[gpui::test]
fn applying_search_keeps_palette_edits_as_a_draft(cx: &mut gpui::TestAppContext) {
    use crate::model::{settings::Settings, state::State};
    use bezel::theme::Appearance;
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window =
        cx.add_window(|window, cx| Cydonia::new(Settings::default(), State::default(), window, cx));
    window
        .update(cx, |this, window, cx| {
            this.toggle_search(&ToggleSearch, window, cx);
            this.search
                .field
                .update(cx, |field, cx| field.set_content("retry", cx));
            this.search.filter = Some(Kind::Article);
            this.apply_search(&ApplySearch, window, cx);
            assert!(!this.search.open);
            assert_eq!(this.applied_query().unwrap().text(), "retry");
            this.toggle_search(&ToggleSearch, window, cx);
            assert_eq!(this.search.field.read(cx).content().as_ref(), "retry");
            assert_eq!(this.search.filter, Some(Kind::Article));
            this.search
                .field
                .update(cx, |field, cx| field.set_content("timeout", cx));
            this.search.filter = Some(Kind::Board);
            this.dismiss_search(&DismissSearch, window, cx);
            assert_eq!(this.applied_query().unwrap().text(), "retry");
            assert_eq!(
                this.search.applied.as_ref().unwrap().filter,
                Some(Kind::Article)
            );
            this.toggle_search(&ToggleSearch, window, cx);
            assert_eq!(this.search.field.read(cx).content().as_ref(), "retry");
            this.clear_applied_search(cx);
            assert!(this.applied_query().is_none());
            assert!(this.applied_rows(cx).is_none());
        })
        .unwrap();
}

#[gpui::test]
fn workspace_search_keeps_results_beyond_the_palette_limit(cx: &mut gpui::TestAppContext) {
    use artifact::project::Project as _;
    let root = std::env::temp_dir().join(format!("cydonia-applied-search-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = fs::Project::new(&root);
    for ix in 0..SHOWN + 1 {
        store
            .create_article(&format!("# Note {ix}\n\nretry request\n\nlater retry"))
            .unwrap();
    }
    let found = search_all(
        std::slice::from_ref(&root),
        &Query::literal("retry").unwrap(),
    );
    assert_eq!(found.len(), SHOWN + 1);
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
            this.workspace.update(cx, |workspace, _| {
                let mut project = crate::model::project::Project::new(root.clone());
                project.expanded = false;
                workspace.projects.push(project);
            });
            let before = this.rows(cx);
            this.search.applied = Some(Applied {
                query: Query::literal("retry").unwrap(),
                filter: Some(Kind::Article),
                found,
                ready: true,
            });
            assert_eq!(
                this.rows(cx)
                    .iter()
                    .filter(|row| matches!(row, Row::Article { .. }))
                    .count(),
                SHOWN + 1
            );
            assert!(!this.workspace.read(cx).projects[0].expanded);
            this.refresh_applied_search(cx);
            assert!(this.search.applied.as_ref().unwrap().ready);
            assert_eq!(this.applied_rows(cx).unwrap().len(), SHOWN + 1);
            let editor = this.workspace.update(cx, |workspace, cx| {
                workspace.active = Some(0);
                workspace.projects[0].article = Some(0);
                workspace.projects[0].articles[0].open(14., cx);
                workspace.projects[0].articles[0].editor.clone().unwrap()
            });
            this.leaf_mut().pane = crate::view::leaf::Pane::Article;
            let matches =
                crate::view::find::hits(editor.read(cx).doc(), this.applied_query().unwrap());
            assert_eq!(matches.len(), 2);
            editor.update(cx, |editor, cx| editor.select(matches[1], cx));
            this.leaf_mut().find_at = 1;
            this.reveal_applied_match(cx);
            assert_eq!(editor.read(cx).selection(), matches[0]);
            assert_eq!(this.leaf().find_at, 0);
            this.search.applied.as_mut().unwrap().filter = Some(Kind::Board);
            assert!(this.rows(cx).is_empty());
            this.clear_applied_search(cx);
            editor.update(cx, |editor, cx| editor.select(matches[1], cx));
            this.reveal_applied_match(cx);
            assert_eq!(editor.read(cx).selection(), matches[1]);
            assert!(this.rows(cx) == before);
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
                .child(div().w(px(240.)).flex_none().child(root.search_row(cx)))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h(px(200.))
                        .children(root.search_pill(None, cx)),
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
        root.update(cx, |root, _| {
            root.leaf_mut().pane = crate::view::leaf::Pane::Board
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
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.search.field.update(cx, |field, cx| {
                    field.set_content(
                        "a very long query that must leave room for the close button",
                        cx,
                    )
                });
                root.apply_search(&ApplySearch, window, cx);
                assert!(!root.board_query(None, cx).is_empty());
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
                root.leaf_mut().finding = true;
                root.leaf()
                    .find_field
                    .update(cx, |field, cx| field.set_content("local query", cx));
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("search-pill").unwrap(), applied_bounds);
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.leaf_mut().finding = false;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let center = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(root.read(cx).applied_query().is_none());
            assert!(root.read(cx).board_query(None, cx).is_empty());
        });
        assert!(cx.debug_bounds("search-pill").is_none());
    }
}

#[test]
fn a_query_reads_as_a_chord_however_it_is_written() {
    let bound = menubar::chord_keys(
        &gpui::Modifiers {
            control: true,
            shift: true,
            ..Default::default()
        },
        "f",
    );
    for query in ["ctrl+shift+f", "Shift Ctrl F", "control-shift-f", "⌃⇧F"] {
        assert_eq!(menubar::query_keys(query), Some(bound.clone()), "{query}");
    }
}

#[test]
fn a_word_or_a_bare_modifier_is_not_a_chord() {
    assert_eq!(menubar::query_keys("find"), None);
    assert_eq!(menubar::query_keys("ctrl"), None);
    assert_eq!(menubar::query_keys("ctrl shift"), None);
}
