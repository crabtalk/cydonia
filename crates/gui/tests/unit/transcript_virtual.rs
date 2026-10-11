use super::*;
use crate::model::{
    project::Project,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Render};
struct ChatView {
    view: gpui::Entity<SessionView>,
    width: f32,
}
impl Render for ChatView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.width))
            .h(px(500.))
            .flex()
            .flex_col()
            .child(self.view.clone())
    }
}
#[gpui::test]
fn long_session_renders_nearby_turns_and_navigates_without_losing_selection(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), state::State::default(), cx));
    workspace.update(cx, |workspace, cx| {
        let cwd = std::path::PathBuf::from("/nonexistent/cydonia-virtual-test");
        let mut project = Project::new(cwd.clone(), cx);
        let record = serde_json::from_value(serde_json::json!({"id":"test", "agent":"test", "title":"", "name":null, "updated":1, "items":[]})).unwrap();
        let mut chat = ChatSession::restore(1, cwd, Agent {name:"test".into(), id:None, command:String::new(), args:vec![], env:Default::default()}, record);
        for i in 0..1000 { chat.items.push(ChatItem::User(format!("Question {i}"))); chat.items.push(ChatItem::Agent(format!("Answer {i}\n\nAnother paragraph with **bold** text."))); }
        project.sessions.push(chat); workspace.projects.push(project);
    });
    let window = cx.add_window(|_, cx| ChatView {
        view: cx.new(|cx| {
            SessionView::new(
                1,
                None,
                Drawn::Pane,
                0.,
                workspace.clone(),
                gpui::WeakEntity::new_invalid(),
                cx,
            )
        }),
        width: 600.,
    });
    let view = window.read_with(cx, |chat, _| chat.view.clone()).unwrap();
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
        view.read_with(&visual, |view, _| {
            let viewport = view.state.list.state.viewport_bounds();
            assert_eq!(viewport.left(), px(0.));
            assert_eq!(viewport.right(), px(width));
            let (position, _) = view.state.layouts.borrow()[&1999]
                .position(
                    markdown::Cursor::new(0, markdown::Part::Body, 0),
                    markdown::Affinity::Downstream,
                )
                .unwrap();
            assert_eq!(
                position.x,
                px(((width - CONTENT_MAX_WIDTH) / 2.).max(0.) + 24.)
            );
        });
    }
    view.read_with(&visual, |view, _| {
        assert!(view.state.layouts.borrow().len() < 100);
        assert!(view.state.list.state.is_following_tail());
        view.state.list.scroll_to(300);
    });
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    view.read_with(&visual, |view, _| {
        assert!(view.state.layouts.borrow().contains_key(&600));
        assert!(view.state.layouts.borrow().len() < 100);
        assert_eq!(view.state.list.state.logical_scroll_top().item_ix, 300);
    });
    workspace.update(&mut visual, |workspace, cx| {
        workspace.with_session(1, cx, |chat| {
            chat.items.push(ChatItem::Agent("More output".into()))
        })
    });
    visual.run_until_parked();
    view.read_with(&visual, |view, _| {
        assert_eq!(view.state.list.state.logical_scroll_top().item_ix, 300)
    });
    let focus = view.update(&mut visual, |view, _| {
        {
            let doc = std::rc::Rc::new(markdown::parse("a selectable reply"));
            view.state.point(
                600,
                selectable::Pointer::Down(
                    markdown::Cursor::new(0, markdown::Part::Body, 0),
                    bezel::ui::input::Granularity::Char,
                ),
                &doc,
            );
            view.state.point(
                600,
                selectable::Pointer::Move(markdown::Cursor::new(0, markdown::Part::Body, 8)),
                &doc,
            );
            view.state.point(600, selectable::Pointer::Up, &doc);
        }
        view.state.focus.borrow().get(&600).unwrap().clone()
    });
    visual.update(|window, cx| window.focus(&focus, cx));
    visual.run_until_parked();
    view.read_with(&visual, |view, _| view.state.list.scroll_to(800));
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
