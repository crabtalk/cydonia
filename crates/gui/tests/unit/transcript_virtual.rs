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
    width: f32,
}
impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.workspace.update(cx, |workspace, cx| {
            super::render(
                workspace.session(1).unwrap(),
                self.width,
                |_, _| Some(div().h(px(30.)).child("Queued message").into_any_element()),
                window,
                cx,
            )
        });
        div()
            .w(px(self.width))
            .h(px(500.))
            .flex()
            .flex_col()
            .child(body)
    }
}
#[gpui::test]
fn long_session_renders_nearby_turns_and_navigates_without_losing_selection(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |workspace, _| {
        let cwd = std::path::PathBuf::from("/nonexistent/cydonia-virtual-test");
        let mut project = Project::new(cwd.clone());
        let record = serde_json::from_value(serde_json::json!({"id":"test", "agent":"test", "title":"", "name":null, "updated":1, "items":[]})).unwrap();
        let mut chat = ChatSession::restore(1, cwd, Agent {name:"test".into(), id:None, command:String::new(), args:vec![], env:Default::default()}, record);
        for i in 0..1000 { chat.items.push(ChatItem::User(format!("Question {i}"))); chat.items.push(ChatItem::Agent(format!("Answer {i}\n\nAnother paragraph with **bold** text."))); }
        project.sessions.push(chat); workspace.projects.push(project);
    });
    let window = cx.add_window(|_, cx| ChatView {
        _watch: cx.observe(&workspace, |_, _, cx| cx.notify()),
        workspace: workspace.clone(),
        width: 600.,
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for width in [1000., 600.] {
        window
            .update(&mut visual, |view, _, cx| {
                view.width = width;
                cx.notify();
            })
            .unwrap();
        visual.simulate_resize(gpui::size(px(width), px(500.)));
        visual.run_until_parked();
        workspace.read_with(&visual, |workspace, _| {
            let chat = workspace.session(1).unwrap();
            let viewport = chat.transcript.list.state.viewport_bounds();
            assert_eq!(viewport.left(), px(0.));
            assert_eq!(viewport.right(), px(width));
            let (position, _) = chat.transcript.layouts.borrow()[&1999]
                .position(markdown::Cursor::new(0, markdown::Part::Body, 0))
                .unwrap();
            assert_eq!(
                position.x,
                px(((width - CONTENT_MAX_WIDTH) / 2.).max(0.) + 24.)
            );
        });
    }
    workspace.read_with(&visual, |workspace, _| {
        let chat = workspace.session(1).unwrap();
        assert!(chat.transcript.layouts.borrow().len() < 100);
        assert!(chat.transcript.list.state.is_following_tail());
        chat.transcript.list.scroll_to(300);
    });
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    workspace.read_with(&visual, |workspace, _| {
        let chat = workspace.session(1).unwrap();
        assert!(chat.transcript.layouts.borrow().contains_key(&600));
        assert!(chat.transcript.layouts.borrow().len() < 100);
        assert_eq!(chat.transcript.list.state.logical_scroll_top().item_ix, 300);
    });
    workspace.update(&mut visual, |workspace, cx| {
        workspace.with_session(1, cx, |chat| {
            chat.items.push(ChatItem::Agent("More output".into()))
        })
    });
    visual.run_until_parked();
    workspace.read_with(&visual, |workspace, _| {
        assert_eq!(
            workspace
                .session(1)
                .unwrap()
                .transcript
                .list
                .state
                .logical_scroll_top()
                .item_ix,
            300
        )
    });
    let focus = workspace.update(&mut visual, |workspace, cx| {
        workspace.with_session(1, cx, |chat| {
            chat.transcript.point(
                600,
                selectable::Pointer::Down(markdown::Cursor::new(0, markdown::Part::Body, 0)),
            );
            chat.transcript.point(
                600,
                selectable::Pointer::Move(markdown::Cursor::new(0, markdown::Part::Body, 8)),
            );
            chat.transcript.point(600, selectable::Pointer::Up);
        });
        workspace
            .session(1)
            .unwrap()
            .transcript
            .focus
            .borrow()
            .get(&600)
            .unwrap()
            .clone()
    });
    visual.update(|window, cx| window.focus(&focus, cx));
    visual.run_until_parked();
    workspace.read_with(&visual, |workspace, _| {
        workspace.session(1).unwrap().transcript.list.scroll_to(800)
    });
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-c");
    visual.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("Question")
        )
    });
}
