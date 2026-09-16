use super::*;
use crate::model::settings::CONTENT_TEXT_SIZE;

#[test]
fn zoom_preserves_base_and_follows_a_changed_default() {
    let mut font = TerminalFont::default();
    font.step(1.);
    font.step(1.);
    assert_eq!(font.base, 13.);
    assert_eq!(font.size(), 15.);
    font.rebase(16.);
    assert_eq!(font.size(), 18.);
    font.adjustment = 0.;
    assert_eq!(font.size(), 16.);
}

#[test]
fn zoom_responds_immediately_after_hitting_either_limit() {
    let mut font = TerminalFont::default();
    for _ in 0..100 {
        font.step(1.);
    }
    assert_eq!(font.size(), CONTENT_TEXT_SIZE.1);
    font.step(-1.);
    assert_eq!(font.size(), CONTENT_TEXT_SIZE.1 - 1.);
    for _ in 0..100 {
        font.step(-1.);
    }
    assert_eq!(font.size(), CONTENT_TEXT_SIZE.0);
    font.step(1.);
    assert_eq!(font.size(), CONTENT_TEXT_SIZE.0 + 1.);
}

#[gpui::test]
fn file_zoom_is_shared_and_independent_of_terminal(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        set_file_size(16., cx);
        set_terminal_size(14., cx);
        zoom_file(2., cx);
        assert_eq!(file_size(cx), 18.);
        assert_eq!(terminal_size(cx), 14.);
        set_file_size(20., cx);
        assert_eq!(file_size(cx), 22.);
        reset_file_zoom(cx);
        assert_eq!(file_size(cx), 20.);
        for _ in 0..100 {
            zoom_file(1., cx);
        }
        zoom_file(-1., cx);
        assert_eq!(file_size(cx), CONTENT_TEXT_SIZE.1 - 1.);
    });
}
