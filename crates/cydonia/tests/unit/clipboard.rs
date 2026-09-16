use bezel::gpui::{AnyView, ClipboardItem, Context, Render, Window, div, prelude::*};

pub struct CopyRoot(pub AnyView);

impl Render for CopyRoot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("Cydonia")
            .size_full()
            .on_action(|_: &super::root::CopySelection, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string("transcript fallback".into()));
            })
            .child(self.0.clone())
    }
}

struct Transcript {
    focus: bezel::gpui::FocusHandle,
}

impl Render for Transcript {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).child("Transcript")
    }
}

#[gpui::test]
fn transcript_copy_fallback_still_works(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| super::keymap::bind_all(&crate::model::settings::Shortcuts::default(), cx));
    let window = cx.add_window(|window, cx| {
        let transcript = cx.new(|cx| Transcript {
            focus: cx.focus_handle(),
        });
        let focus = transcript.read(cx).focus.clone();
        window.focus(&focus, cx);
        CopyRoot(transcript.into())
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-c");
    visual.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("transcript fallback")
        );
    });
}
