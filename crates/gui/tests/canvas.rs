use artifact::project::{Project as _, memory};
use bezel::gpui::{AppContext as _, Entity, TestAppContext};
use canvas::{Canvas, CanvasView, Change, layout};
use cydonia_gui::model::{canvas::Canvas as HeldCanvas, store::Store};
use std::sync::Arc;

fn open(cx: &mut TestAppContext) -> (Store, Entity<HeldCanvas>) {
    let store = Store::Memory(Arc::new(memory::Project::new()));
    let document = Canvas::parse(r#"{"nodes":[{"id":"a","type":"text","x":0,"y":0,"width":200,"height":100,"text":"original"}]}"#).unwrap();
    let record = store.create_canvas("Map", &document).unwrap();
    let held = cx.update(|cx| cx.new(|cx| HeldCanvas::new(record, store.clone(), cx)));
    (store, held)
}

#[gpui::test]
fn edits_autosave_and_reach_another_pane(cx: &mut TestAppContext) {
    let (store, held) = open(cx);
    let document = held.read_with(cx, |held, _| held.document.clone());
    let (first, second) = cx.update(|cx| {
        (
            cx.new(|cx| CanvasView::for_document(document.clone(), layout::FREE, cx)),
            cx.new(|cx| CanvasView::for_document(document.clone(), layout::FREE, cx)),
        )
    });
    first.update(cx, |view, cx| {
        view.update_editor(cx, |editor| {
            editor.select(Some("a".into()));
            editor.nudge(layout::Arrow::Right);
        })
    });
    cx.run_until_parked();
    let id = held.read_with(cx, |held, _| {
        assert!(held.error.is_none());
        held.record.id.clone()
    });
    assert_eq!(store.canvas(&id).unwrap().document.nodes[0].x, 8);
    second.read_with(cx, |view, _| {
        assert_eq!(view.editor().canvas().nodes[0].x, 8)
    });
}

#[gpui::test]
fn stale_saves_preserve_the_local_document_until_the_user_chooses(cx: &mut TestAppContext) {
    let (store, held) = open(cx);
    let id = held.read_with(cx, |held, _| held.record.id.clone());
    let mut external = store.canvas(&id).unwrap();
    external.document.nodes[0].text = Some("external".into());
    store.save_canvas(&mut external).unwrap();
    let document = held.read_with(cx, |held, _| held.document.clone());
    let mut node = document.read_with(cx, |document, _| document.nodes[0].clone());
    node.text = Some("local".into());
    document.update(cx, |document, cx| {
        document.edit(vec![Change::UpdateNode { node }], cx)
    });
    cx.run_until_parked();
    assert_eq!(
        store.canvas(&id).unwrap().document.nodes[0].text.as_deref(),
        Some("external")
    );
    held.update(cx, |held, cx| held.adopt(external, cx));
    held.read_with(cx, |held, cx| {
        assert!(held.error.is_some());
        assert_eq!(
            held.document.read(cx).nodes[0].text.as_deref(),
            Some("local")
        );
    });
    held.update(cx, |held, cx| held.keep(cx));
    cx.run_until_parked();
    assert_eq!(
        store.canvas(&id).unwrap().document.nodes[0].text.as_deref(),
        Some("local")
    );
    held.read_with(cx, |held, _| assert!(held.error.is_none()));
}

#[gpui::test]
fn failed_metadata_changes_do_not_change_the_entry_in_memory(cx: &mut TestAppContext) {
    let (store, held) = open(cx);
    let id = held.read_with(cx, |held, _| held.record.id.clone());
    let mut external = store.canvas(&id).unwrap();
    external.title = "External title".into();
    store.save_canvas(&mut external).unwrap();
    held.update(cx, |held, cx| {
        assert!(!held.rename("Local title".into(), cx));
        assert_eq!(held.record.title, "Map");
        assert!(held.error.is_some());
        held.reload(cx);
        assert_eq!(held.record.title, "External title");
        assert!(held.error.is_none());
    });
}

#[gpui::test]
fn converting_a_card_creates_an_article_and_keeps_canvas_undo(cx: &mut TestAppContext) {
    let (store, held) = open(cx);
    let document = held.read_with(cx, |held, _| held.document.clone());
    let pane = cx.update(|cx| cx.new(|cx| CanvasView::for_document(document, layout::FREE, cx)));
    let article = held.update(cx, |held, cx| {
        held.convert_to_article("a", "project", cx).unwrap()
    });
    cx.run_until_parked();
    assert_eq!(store.read_article(&article.id).unwrap(), "original");
    held.read_with(cx, |held, cx| {
        let node = &held.document.read(cx).nodes[0];
        assert_eq!(node.kind, "link");
        assert!(node.text.is_none());
        assert!(
            node.url
                .as_deref()
                .unwrap()
                .starts_with("cydonia://project#")
        );
        assert_eq!((node.width, node.height), (200, 100));
    });
    pane.update(cx, |pane, cx| {
        pane.update_editor(cx, |editor| {
            editor.undo();
        })
    });
    cx.run_until_parked();
    held.read_with(cx, |held, _| {
        assert_eq!(
            held.record.document.nodes[0].text.as_deref(),
            Some("original")
        )
    });
    assert_eq!(store.read_article(&article.id).unwrap(), "original");
}

#[gpui::test]
fn failed_conversion_keeps_the_text_and_does_not_leave_an_article(cx: &mut TestAppContext) {
    let (store, held) = open(cx);
    let id = held.read_with(cx, |held, _| held.record.id.clone());
    let mut external = store.canvas(&id).unwrap();
    external.title = "another writer".into();
    store.save_canvas(&mut external).unwrap();
    assert!(
        held.update(cx, |held, cx| held.convert_to_article("a", "project", cx))
            .is_err()
    );
    assert!(store.articles().is_empty());
    held.read_with(cx, |held, cx| {
        assert_eq!(
            held.document.read(cx).nodes[0].text.as_deref(),
            Some("original")
        )
    });
}
