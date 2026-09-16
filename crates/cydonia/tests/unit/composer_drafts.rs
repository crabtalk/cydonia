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
