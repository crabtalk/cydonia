//! The list's thumb under the "while scrolling" setting, on a real transcript.

use super::*;
use crate::model::{
    project::Project,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Render};

struct ChatView {
    workspace: gpui::Entity<Workspace>,
    _watch: gpui::Subscription,
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.workspace.update(cx, |workspace, cx| {
            super::render(workspace.session(1).unwrap(), 800., |_, _| None, window, cx)
        });
        div().w(px(800.)).h(px(500.)).flex().flex_col().child(body)
    }
}

fn thumb_painted(cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, _| {
        let scale = window.scale_factor();
        window
            .painted_quads()
            .into_iter()
            .any(|quad| quad.bounds.size.width.0 / scale == 4.)
    })
}

#[gpui::test]
fn a_settled_transcript_hides_its_thumb(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Light, cx);
        bezel::ui::scroll::set_visibility(bezel::ui::scroll::Visibility::Scrolling, cx);
    });
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |workspace, _| {
        let cwd = std::path::PathBuf::from("/nonexistent/cydonia-scrollbar-test");
        let mut project = Project::new(cwd.clone());
        let record = serde_json::from_value(serde_json::json!({"id":"test", "agent":"test", "title":"", "name":null, "updated":1, "items":[]})).unwrap();
        let mut chat = ChatSession::restore(
            1,
            cwd,
            Agent {
                name: "test".into(),
                id: None,
                command: String::new(),
                args: vec![],
                env: Default::default(),
            },
            record,
        );
        for i in 0..200 {
            chat.items.push(ChatItem::User(format!("Question {i}")));
            chat.items.push(ChatItem::Agent(format!(
                "Answer {i}\n\nAnother paragraph with **bold** text."
            )));
        }
        project.sessions.push(chat);
        workspace.projects.push(project);
    });
    let window = cx.add_window(|_, cx| ChatView {
        _watch: cx.observe(&workspace, |_, _, cx| cx.notify()),
        workspace: workspace.clone(),
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    // The bar's geometry is the previous frame's, so the thumb arrives on the
    // frame after the list is first laid out.
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    assert!(
        thumb_painted(&mut visual),
        "the thumb does not show for the scroll that reveals the transcript"
    );
    for _ in 0..3 {
        visual
            .executor()
            .advance_clock(bezel::ui::scroll::TRANSIENT_IDLE);
        visual.update(|window, _| window.refresh());
        visual.run_until_parked();
    }
    assert!(
        !thumb_painted(&mut visual),
        "an idle transcript keeps painting its thumb"
    );
}
