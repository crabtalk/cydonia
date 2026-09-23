use bezel::{
    gpui::{
        self, Context, Entity, Focusable, Render, TestAppContext, VisualTestContext, Window, div,
        prelude::*, px, size,
    },
    theme::{Appearance, Theme},
    ui::input::TextField,
};
use cydonia::{
    model::settings::Shortcuts,
    view::{board, component::terminal::keystroke_bytes, keymap},
};
use terminal::{emulator::KeyboardMode, view::KeyEvent};

/// A keyboard with no protocol enhancements, only DECCKM.
fn legacy(app_cursor: bool) -> KeyboardMode {
    KeyboardMode {
        app_cursor,
        ..KeyboardMode::default()
    }
}

#[test]
fn emacs_preference_preserves_existing_shortcuts() {
    let original: Shortcuts = toml::from_str("toggle_sidebar = 'cmd-j'").unwrap();
    assert!(!original.emacs);
    let mut enabled = original.clone();
    enabled.emacs = true;
    let saved = toml::to_string(&enabled).unwrap();
    let restored: Shortcuts = toml::from_str(&saved).unwrap();
    assert!(restored.emacs);
    assert_eq!(restored.get("toggle_sidebar"), Some("cmd-j"));
    enabled.emacs = false;
    assert!(!toml::to_string(&enabled).unwrap().contains("emacs"));
}

#[test]
fn terminal_meta_uses_letters_instead_of_option_characters() {
    for (chord, character, expected) in [
        ("alt-b", "∫", "\x1bb"),
        ("alt-f", "ƒ", "\x1bf"),
        ("alt-d", "∂", "\x1bd"),
        ("alt-shift-b", "ı", "\x1bB"),
    ] {
        let mut key = gpui::Keystroke::parse(chord).unwrap();
        key.key_char = Some(character.into());
        for app_cursor in [false, true] {
            assert_eq!(
                keystroke_bytes(&key, legacy(app_cursor), KeyEvent::Press),
                Some(expected.as_bytes().to_vec())
            );
        }
    }
}

#[test]
fn terminal_preserves_text_control_and_app_shortcuts() {
    let mut key = gpui::Keystroke::parse("a").unwrap();
    key.key_char = Some("文".into());
    assert_eq!(
        keystroke_bytes(&key, legacy(false), KeyEvent::Press),
        Some("文".as_bytes().to_vec())
    );
    for (chord, expected) in [
        ("ctrl-b", Some(b"\x02".to_vec())),
        ("alt-ctrl-b", Some(b"\x1b\x02".to_vec())),
        ("alt-backspace", Some(b"\x1b\x7f".to_vec())),
        ("cmd-b", None),
        ("alt-cmd-b", None),
        ("alt-f13", None),
    ] {
        assert_eq!(
            keystroke_bytes(
                &gpui::Keystroke::parse(chord).unwrap(),
                legacy(false),
                KeyEvent::Press
            ),
            expected
        );
    }
}

struct CardEditor(Entity<TextField>);

impl Render for CardEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<TextField>, VisualTestContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        let mut shortcuts = Shortcuts::default();
        shortcuts.emacs = true;
        keymap::bind_all(&shortcuts, cx);
    });
    let window = cx.add_window(|_, cx| CardEditor(board::field(cx)));
    let field = window
        .root(cx)
        .unwrap()
        .read_with(cx, |host, _| host.0.clone());
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(300.), px(200.)));
    visual.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.set_content("one two three", cx);
            field.focus_handle(cx).focus(window, cx);
        })
    });
    visual.run_until_parked();
    (field, visual)
}

fn option_letter(cx: &mut VisualTestContext, chord: &str, character: &str) {
    let mut key = gpui::Keystroke::parse(chord).unwrap();
    key.key_char = Some(character.into());
    cx.update(|window, cx| window.dispatch_keystroke(key, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn card_emacs_movement_selection_and_deletion(cx: &mut TestAppContext) {
    let (field, mut cx) = open(cx);
    cx.simulate_keystrokes("end");
    option_letter(&mut cx, "alt-b", "∫");
    assert_eq!(field.read_with(&cx, |field, _| field.cursor()), 8);
    cx.simulate_keystrokes("alt-b alt-f");
    assert_eq!(field.read_with(&cx, |field, _| field.cursor()), 7);
    cx.simulate_keystrokes("end alt-shift-b");
    cx.simulate_input("last");
    assert_eq!(
        field.read_with(&cx, |field, _| field.content().to_string()),
        "one two last"
    );
    cx.simulate_keystrokes("alt-b alt-d");
    assert_eq!(
        field.read_with(&cx, |field, _| field.content().to_string()),
        "one two "
    );
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn disabling_emacs_restores_option_text_and_keeps_native_word_motion(cx: &mut TestAppContext) {
    let (field, mut cx) = open(cx);
    cx.simulate_keystrokes("end alt-left");
    assert_eq!(field.read_with(&cx, |field, _| field.cursor()), 8);
    cx.update(|_, cx| keymap::bind_all(&Shortcuts::default(), cx));
    cx.simulate_keystrokes("end");
    option_letter(&mut cx, "alt-b", "∫");
    assert_eq!(
        field.read_with(&cx, |field, _| field.content().to_string()),
        "one two three∫"
    );
    cx.simulate_keystrokes("home alt-right");
    assert_eq!(field.read_with(&cx, |field, _| field.cursor()), 3);
}

#[gpui::test]
fn article_emacs_shortcuts_work_in_both_editor_modes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::install(Appearance::Dark, cx);
        let mut shortcuts = Shortcuts::default();
        shortcuts.emacs = true;
        keymap::bind_all(&shortcuts, cx);
    });
    for mode in [editor::Mode::Blocks, editor::Mode::Source] {
        let window =
            cx.add_window(|_, cx| editor::Editor::new("one two three", cx).with_mode(mode));
        let editor = window.root(cx).unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.simulate_resize(size(px(400.), px(300.)));
        visual.update(|window, cx| editor.read(cx).focus_handle(cx).focus(window, cx));
        visual.run_until_parked();
        visual.simulate_keystrokes("home");
        option_letter(&mut visual, "alt-f", "ƒ");
        visual.simulate_input("!");
        assert_eq!(
            editor.read_with(&visual, |editor, _| editor.source()),
            "one! two three"
        );
        visual.simulate_keystrokes("end alt-shift-b");
        visual.simulate_input("last");
        assert_eq!(
            editor.read_with(&visual, |editor, _| editor.source()),
            "one! two last"
        );
        visual.simulate_keystrokes("alt-b alt-d");
        assert_eq!(
            editor
                .read_with(&visual, |editor, _| editor.source())
                .trim_end(),
            "one! two"
        );
    }
}
