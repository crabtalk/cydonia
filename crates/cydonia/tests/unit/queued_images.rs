use super::*;
use crate::model::{
    project::Project,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Entity, Render};

/// The queue of one session, drawn the way a pane on it draws it.
struct QueueView(Entity<Cydonia>, u64);

impl Render for QueueView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on = self.1;
        let queue = self.0.update(cx, |root, cx| root.queue(on, None, window, cx));
        div().w(px(600.)).children(queue)
    }
}

#[gpui::test]
fn pending_image_only_message_has_a_gallery_and_retains_preview(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        let root =
            cx.new(|cx| Cydonia::new(Settings::default(), state::State::default(), window, cx));
        root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, _| {
                workspace.settings.features.sessions = true;
                let cwd = std::path::PathBuf::from("/nonexistent/cydonia-queue-test");
                let mut project = Project::new(cwd.clone());
                let record = serde_json::from_value(serde_json::json!({
                    "id":"test", "agent":"test", "title":"", "name":null, "updated":1, "items":[]
                }))
                .unwrap();
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
                chat.queue.push_back("![](</tmp/queued.png>)".into());
                project.sessions.push(chat);
                project.active = Some(1);
                workspace.projects.push(project);
                workspace.active = Some(0);
            });
        });
        QueueView(root, 1)
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let image = visual
        .debug_bounds("sent-image")
        .expect("pending image gallery");
    assert!(image.size.width > px(300.));
    assert!(image.size.height > px(0.));
    visual.simulate_click(image.center(), gpui::Modifiers::default());
    visual.run_until_parked();
    assert!(visual.debug_bounds("sent-image-preview").is_some());
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    assert!(visual.debug_bounds("sent-image-preview").is_some());
}

/// A pane draws the queue of the session it is on and no other.
///
/// Two sessions in one window, one with a message waiting: the pane on the
/// other must draw nothing. Drawn off the window's active session instead,
/// every pane of a space shows the same bubble — which reads as one message
/// queued to every agent.
#[gpui::test]
fn a_queue_is_drawn_only_under_the_session_it_is_waiting_in(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        let root =
            cx.new(|cx| Cydonia::new(Settings::default(), state::State::default(), window, cx));
        root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, _| {
                workspace.settings.features.sessions = true;
                let cwd = std::path::PathBuf::from("/nonexistent/cydonia-queue-test");
                let mut project = Project::new(cwd.clone());
                for id in [1, 2] {
                    let record = serde_json::from_value(serde_json::json!({
                        "id":"test", "agent":"test", "title":"", "name":null,
                        "updated":1, "items":[]
                    }))
                    .unwrap();
                    let mut chat = ChatSession::restore(
                        id,
                        cwd.clone(),
                        Agent {
                            name: "test".into(),
                            id: None,
                            command: String::new(),
                            args: vec![],
                            env: Default::default(),
                        },
                        record,
                    );
                    // Only the first is waiting on anything.
                    if id == 1 {
                        chat.queue.push_back("hold this".into());
                    }
                    project.sessions.push(chat);
                }
                // And the window is on the one that is.
                project.active = Some(1);
                workspace.projects.push(project);
                workspace.active = Some(0);
            });
        });
        QueueView(root, 1)
    });
    window
        .update(cx, |view, window, cx| {
            let root = view.0.clone();
            root.update(cx, |root, cx| {
                assert!(
                    root.queue(1, None, window, cx).is_some(),
                    "the session holding it draws it"
                );
                assert!(
                    root.queue(2, None, window, cx).is_none(),
                    "and the session beside it draws nothing"
                );
            });
        })
        .unwrap();
}
