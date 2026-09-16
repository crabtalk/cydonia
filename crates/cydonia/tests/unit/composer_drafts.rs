use super::*;
use bezel::theme::Appearance;
use gpui::TestAppContext;

#[gpui::test]
fn switching_to_a_fork_keeps_drafts_and_attachments_separate(cx: &mut TestAppContext) {
    let composer = cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        cx.new(Composer::new)
    });
    composer.update(cx, |composer, cx| {
        composer.set_session(Some(1), "original draft", cx);
        composer
            .attachments
            .push(Attachment::File("/tmp/original.png".into()));
        composer.set_session(Some(2), "fork draft", cx);
        assert_eq!(composer.field.read(cx).content().as_ref(), "fork draft");
        assert!(composer.attachments.is_empty());
        composer
            .field
            .update(cx, |field, cx| field.set_content("edited fork", cx));
        composer.set_session(Some(2), "fork draft", cx);
        assert_eq!(composer.field.read(cx).content().as_ref(), "edited fork");
        composer.set_session(Some(1), "original draft", cx);
        assert_eq!(composer.field.read(cx).content().as_ref(), "original draft");
        assert_eq!(composer.attachments.len(), 1);
        composer.set_session(Some(2), "edited fork", cx);
        assert!(composer.attachments.is_empty());
        assert_eq!(composer.field.read(cx).content().as_ref(), "edited fork");
    });
}

#[gpui::test]
fn macos_word_navigation_and_selection(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        crate::view::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx);
    });
    let composer = cx.new(Composer::new);
    composer.update(cx, |composer, cx| {
        composer.set_session(Some(1), "one two three", cx)
    });
    let window = cx.add_window(|window, cx| {
        window.focus(&composer.focus_handle(cx), cx);
        crate::view::clipboard_tests::CopyRoot(composer.clone().into())
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-right alt-left");
    composer.read_with(&visual, |composer, cx| {
        assert_eq!(composer.field.read(cx).cursor(), 8)
    });
    visual.simulate_keystrokes("alt-left");
    composer.read_with(&visual, |composer, cx| {
        assert_eq!(composer.field.read(cx).cursor(), 4)
    });
    visual.simulate_keystrokes("alt-right");
    composer.read_with(&visual, |composer, cx| {
        assert_eq!(composer.field.read(cx).cursor(), 7)
    });
    visual.simulate_keystrokes("alt-shift-right cmd-c");
    visual.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some(" three")
        )
    });
    visual.simulate_keystrokes("alt-backspace");
    composer.read_with(&visual, |composer, cx| {
        assert_eq!(composer.field.read(cx).content().as_ref(), "one two")
    });
}
