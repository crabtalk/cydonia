use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "cydonia-file-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, "original\r\n").unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn saves_refuse_to_overwrite_external_edits_or_deleted_files() {
    let file = Temp::new();
    std::fs::write(&file.0, "agent edit").unwrap();
    assert!(write_text(&file.0, "original\r\n", "user edit", false).is_err());
    assert_eq!(read_text(&file.0).unwrap(), "agent edit");
    write_text(&file.0, "original\r\n", "user edit", true).unwrap();
    assert_eq!(read_text(&file.0).unwrap(), "user edit");
    std::fs::remove_file(&file.0).unwrap();
    assert!(write_text(&file.0, "user edit", "new", false).is_err());
    assert!(!file.0.exists());
}

#[test]
fn text_loading_rejects_binary_invalid_utf8_and_large_files() {
    let file = Temp::new();
    for bytes in [vec![0], vec![255], vec![b'a'; LIMIT as usize + 1]] {
        std::fs::write(&file.0, bytes).unwrap();
        assert!(read_text(&file.0).is_err());
    }
}

#[gpui::test]
fn clean_buffers_reload_but_dirty_buffers_keep_local_edits(cx: &mut gpui::TestAppContext) {
    let file = Temp::new();
    let view = cx.new(|cx| FileView::new(file.0.clone(), cx));
    view.update(cx, |view, cx| {
        view.receive(Ok("original\r\n".into()), cx);
        assert!(!view.dirty(cx));
        view.receive(Ok("external\r\n".into()), cx);
        assert_eq!(view.field.read(cx).content().as_ref(), "external\n");
        view.field
            .update(cx, |field, cx| field.set_content("local\n", cx));
        view.receive(Ok("another edit\r\n".into()), cx);
        assert!(view.changed);
        assert!(view.dirty(cx));
        assert_eq!(view.field.read(cx).content().as_ref(), "local\n");
    });
}

#[gpui::test]
fn saving_preserves_crlf_and_clears_dirty_state(cx: &mut gpui::TestAppContext) {
    let file = Temp::new();
    let view = cx.new(|cx| FileView::new(file.0.clone(), cx));
    view.update(cx, |view, cx| {
        view.receive(Ok("original\r\n".into()), cx);
        assert!(!view.dirty(cx));
        view.field
            .update(cx, |field, cx| field.set_content("edited\n", cx));
        assert!(view.save(false, cx));
        assert!(!view.dirty(cx));
        assert_eq!(read_text(&file.0).unwrap(), "edited\r\n");
    });
}

#[test]
fn line_numbers_follow_logical_lines_and_utf8_offsets() {
    assert_eq!(line_starts(""), vec![0]);
    assert_eq!(line_starts("a\n\n你好\n"), vec![0, 2, 3, 10]);
}

#[gpui::test]
fn source_uses_full_viewport_and_scrolls_long_files(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let file = Temp::new();
    let source = (0..100).map(|n| format!("line {n}\n")).collect::<String>();
    std::fs::write(&file.0, &source).unwrap();
    let window = cx.add_window(|_, cx| {
        let mut view = FileView::new(file.0.clone(), cx);
        view.receive(Ok(source.clone()), cx);
        view
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(gpui::size(px(500.), px(1000.)));
    visual.run_until_parked();
    window
        .update(&mut visual, |view, _, cx| {
            let viewport = view.scroll.bounds();
            assert!(viewport.size.height > px(900.));
            let starts = line_starts(view.field.read(cx).content());
            let line35 = view.field.read(cx).offset_bounds(starts[35]).unwrap();
            assert!(line35.top() > viewport.top() + viewport.size.height / 2.);
            assert!(line35.bottom() < viewport.bottom());
            assert!(view.scroll.max_offset().y > px(0.));
        })
        .unwrap();
    visual.simulate_resize(gpui::size(px(300.), px(400.)));
    visual.run_until_parked();
    window
        .update(&mut visual, |view, _, _| {
            assert!(view.scroll.bounds().size.height <= px(400.));
            assert!(view.scroll.max_offset().y > px(1000.));
        })
        .unwrap();
}
