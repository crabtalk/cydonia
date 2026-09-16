use super::*;
use crate::model::{
    project::Project,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Entity, Render};

struct QueueView(Entity<Cydonia>);

impl Render for QueueView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let queue = self.0.update(cx, |root, cx| root.queue(window, cx));
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
        QueueView(root)
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
