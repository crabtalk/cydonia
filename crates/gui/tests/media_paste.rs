//! A picture's web address pasted into a document.

use cydonia_gui::model::media::Pasting;
use editor::{Editor, Mode};
use gpui::{ClipboardItem, Focusable, TestAppContext, VisualTestContext};

#[cfg(target_os = "macos")]
const PASTE: &str = "cmd-v";
#[cfg(not(target_os = "macos"))]
const PASTE: &str = "ctrl-v";

/// Pictures pasted as pictures, nothing fetched.
const ON: Pasting = Pasting {
    fetch: false,
    source: true,
};

const WEB: &str = "https://example.com/picture.png";

fn pasted(mode: Mode, pasting: Pasting, cx: &mut TestAppContext) -> String {
    cx.update(|cx| {
        bezel::theme::Theme::install(bezel::theme::Appearance::Dark, cx);
        editor::init(cx);
        cydonia_gui::model::media::init(pasting, cx);
    });
    let window = cx.add_window(|_, cx| Editor::new("", cx).with_mode(mode).with_base("/notes"));
    let editor = window.root(cx).unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, cx| window.focus(&editor.read(cx).focus_handle(cx), cx));
    cx.run_until_parked();
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(WEB.to_owned())));
    cx.simulate_keystrokes(PASTE);
    cx.run_until_parked();
    cx.update(|_, cx| editor.read(cx).source())
}

#[gpui::test]
fn a_picture_link_pasted_in_source_is_a_picture(cx: &mut TestAppContext) {
    assert_eq!(pasted(Mode::Source, ON, cx).trim(), format!("![]({WEB})"));
}

#[gpui::test]
fn a_picture_link_pasted_in_rich_text_is_a_picture(cx: &mut TestAppContext) {
    assert_eq!(pasted(Mode::Blocks, ON, cx).trim(), format!("![]({WEB})"));
}

#[gpui::test]
fn off_source_pastes_the_link_as_text(cx: &mut TestAppContext) {
    let off = Pasting {
        source: false,
        ..ON
    };
    assert_eq!(pasted(Mode::Source, off, cx).trim(), WEB);
}
