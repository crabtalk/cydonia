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

struct PopoverComposer(Entity<Composer>);

impl Render for PopoverComposer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_end()
            .child(self.0.clone())
    }
}

#[gpui::test]
fn agent_icon_closes_its_open_popover_and_can_open_it_again(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Light, cx));
    let composer = cx.new(Composer::new);
    composer.update(cx, |composer, cx| {
        composer.set_agents(
            &[Agent {
                name: "Codex".into(),
                icon: None,
            }],
            Some(0),
            cx,
        );
    });
    let window = cx.add_window(|_, _| PopoverComposer(composer.clone()));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for open in [true, false, true, false] {
        let point = visual.debug_bounds("composer-agent").unwrap().center();
        visual.simulate_click(point, gpui::Modifiers::default());
        visual.run_until_parked();
        assert_eq!(
            composer.read_with(&visual, |composer, _| composer.menu),
            open
        );
    }
}

#[gpui::test]
fn composer_preview_uses_the_shared_close_button(cx: &mut TestAppContext) {
    cx.update(|cx| Theme::install(Appearance::Light, cx));
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(80, 80, image::Rgba([255, 255, 255, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let picture = Arc::new(gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        png.into_inner(),
    ));
    let composer = cx.new(|cx| {
        let mut composer = Composer::new(cx);
        composer.attachments.push(Attachment::Bytes(picture));
        composer.preview = Some(0);
        composer
    });
    let window = cx.add_window(|_, _| PopoverComposer(composer.clone()));
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let button = visual
        .debug_bounds("composer-preview-close")
        .expect("close button is painted");
    visual.simulate_click(button.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    assert_eq!(
        composer.read_with(&visual, |composer, _| composer.preview),
        None
    );
}
