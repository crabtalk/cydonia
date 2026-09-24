use super::*;
use bezel::gpui::{TestAppContext, VisualTestContext, size};
use bezel::theme::Appearance;

#[test]
fn revealing_moves_only_the_hidden_edge() {
    assert_eq!(reveal_delta(px(40.), px(100.), px(20.), px(200.)), px(0.));
    assert_eq!(
        reveal_delta(px(170.), px(230.), px(20.), px(200.)),
        px(-30.)
    );
    assert_eq!(reveal_delta(px(10.), px(70.), px(20.), px(200.)), px(10.));
}

#[test]
fn a_card_taller_than_the_exposed_lane_aligns_at_the_top() {
    assert_eq!(reveal_delta(px(60.), px(240.), px(20.), px(100.)), px(-40.));
    assert_eq!(reveal_delta(px(20.), px(200.), px(20.), px(100.)), px(0.));
}

#[test]
fn closing_restores_automatic_scrolling_but_preserves_manual_scrolling() {
    let scroll = ScrollHandle::new();
    let before = gpui::point(px(0.), px(-30.));
    let after = gpui::point(px(0.), px(-120.));
    let adjustment = LaneAdjustment {
        scroll: scroll.clone(),
        before,
        after,
    };
    scroll.set_offset(after);
    adjustment.restore();
    assert_eq!(scroll.offset(), before);

    let manual = gpui::point(px(0.), px(-160.));
    scroll.set_offset(manual);
    adjustment.restore();
    assert_eq!(scroll.offset(), manual);
}

struct Preview {
    doc: markdown::Doc,
    overflow: Entity<bool>,
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(240.)).child(card_preview(
            &self.doc,
            false,
            self.overflow.clone(),
            window,
            cx,
        ))
    }
}

#[gpui::test]
fn rendered_overflow_updates_when_a_card_is_shortened(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Dark, cx));
    let window = cx.add_window(|_, cx| Preview {
        doc: markdown::parse(&"a paragraph\n\n".repeat(30)),
        overflow: cx.new(|_| false),
    });
    let page = window.root(cx).unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(300.), px(400.)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.update(|_, cx| *page.read(cx).overflow.read(cx)));

    cx.update(|_, cx| {
        page.update(cx, |page, cx| {
            page.doc = markdown::parse("Short card");
            cx.notify();
        });
    });
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| *page.read(cx).overflow.read(cx)));
}

#[test]
fn lane_preview_bounds_large_documents_without_changing_full_content() {
    let docs = Docs::default();
    let source = "A paragraph.\n\n".repeat(1000);
    let full = docs.of(&source);
    let (preview, shortened) = docs.preview(&source);
    assert!(shortened);
    assert_eq!(preview.blocks.len(), 16);
    assert_eq!(full.blocks.len(), 1000);
    assert!(Rc::ptr_eq(&full, &docs.of(&source)));
    assert!(Rc::ptr_eq(&preview, &docs.preview(&source).0));

    let source = format!("```rust\n{}\n```", "let answer = 42;\n".repeat(1000));
    let (preview, shortened) = docs.preview(&source);
    assert!(shortened);
    assert!(
        preview.blocks[0]
            .text_at(markdown::Part::Code)
            .unwrap()
            .text
            .lines()
            .count()
            <= 17
    );
    assert!(
        docs.of(&source).blocks[0]
            .text_at(markdown::Part::Code)
            .unwrap()
            .text
            .lines()
            .count()
            >= 1000
    );
}

#[test]
fn bounded_preview_preserves_unicode_marks_and_short_documents() {
    let docs = Docs::default();
    let source = format!("**{}**", "界".repeat(5000));
    let (preview, shortened) = docs.preview(&source);
    assert!(shortened);
    let text = preview.blocks[0].text_at(markdown::Part::Body).unwrap();
    assert_eq!(text.text.chars().count(), 4096);
    assert!(
        text.marks
            .iter()
            .all(|span| span.range.end <= text.text.len())
    );
    assert_eq!(text.marks[0].range.end, text.text.len());

    let source = "# Heading\n\nA **short** card.";
    let (preview, shortened) = docs.preview(source);
    assert!(!shortened);
    assert_eq!(preview, docs.of(source));
}

#[test]
fn lane_window_is_bounded_at_board_edges() {
    assert_eq!(visible_lanes(px(0.), px(700.), 20), 0..4);
    assert_eq!(visible_lanes(px(-2720.), px(700.), 20), 8..14);
    assert_eq!(visible_lanes(px(-9000.), px(700.), 20), 20..20);
    assert_eq!(visible_lanes(px(0.), px(700.), 0), 0..0);
}

#[test]
fn preview_limits_table_rows_but_keeps_the_full_table() {
    let source = format!("| Column |\n| --- |\n{}", "| cell |\n".repeat(1000));
    let docs = Docs::default();
    let (preview, shortened) = docs.preview(&source);
    assert!(shortened);
    let markdown::BlockKind::Table { rows, .. } = &preview.blocks[0].kind else {
        panic!("table expected")
    };
    assert_eq!(rows.len(), 12);
    let full = docs.of(&source);
    let markdown::BlockKind::Table { rows, .. } = &full.blocks[0].kind else {
        panic!("table expected")
    };
    assert_eq!(rows.len(), 1000);
}
