use super::*;
use crate::model::{
    project::Project,
    session::ChatSession,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Entity, Render};

struct BarView {
    root: Entity<Cydonia>,
    stack: Vec<Member>,
    _watch: gpui::Subscription,
}

impl Render for BarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::of(cx).clone();
        let bar = self.root.update(cx, |root, cx| {
            root.pane_bar(
                &self.stack[0],
                &self.stack,
                &self.stack[0],
                false,
                false,
                &theme,
                window,
                cx,
            )
        });
        div().w(px(320.)).h(px(200.)).child(bar)
    }
}

#[gpui::test]
fn scrollbar_hover_keeps_pane_controls_and_geometry_stable(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        let root = cx.new(|cx| Cydonia::new(Settings::default(), state::State::default(), window, cx));
        let stack = root.update(cx, |root, cx| {
            root.workspace.update(cx, |workspace, _| {
                let cwd = std::path::PathBuf::from("/nonexistent/cydonia-pane-hover");
                let mut project = Project::new(cwd.clone());
                for id in 1..=12 {
                    let record = serde_json::from_value(serde_json::json!({
                        "id":id.to_string(), "agent":"test", "title":"A long session title", "name":null, "updated":1, "items":[]
                    })).unwrap();
                    project.sessions.push(ChatSession::restore(id, cwd.clone(), Agent {
                        name: "test".into(), id: None, command: String::new(), args: vec![], env: Default::default(),
                    }, record));
                }
                workspace.projects.push(project);
                workspace.active = Some(0);
                (1..=12).map(|id| workspace.member_of(0, Showing::Session(id)).unwrap()).collect()
            })
        });
        BarView { _watch: cx.observe(&root, |_, _, cx| cx.notify()), root, stack }
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let key = window
        .read_with(&visual, |view, _| key_of(&view.stack[0]))
        .unwrap();
    visual.simulate_mouse_move(
        gpui::point(px(100.), px(12.)),
        None,
        gpui::Modifiers::default(),
    );
    visual.run_until_parked();
    visual.update(|window, _| window.refresh());
    visual.run_until_parked();
    let selector: &'static str =
        Box::leak(format!("strip-bar-pane-strip-{key}-track").into_boxed_str());
    let track = visual.debug_bounds(selector).expect("overflow scrollbar");
    visual.simulate_mouse_move(track.center(), None, gpui::Modifiers::default());
    for _ in 0..6 {
        visual.update(|window, _| window.refresh());
        visual.run_until_parked();
        window
            .read_with(&visual, |view, cx| {
                assert_eq!(view.root.read(cx).pane_hovered.as_ref(), Some(&key));
            })
            .unwrap();
        assert_eq!(visual.debug_bounds(selector), Some(track));
    }
    visual.simulate_mouse_move(
        gpui::point(px(100.), px(100.)),
        None,
        gpui::Modifiers::default(),
    );
    visual.run_until_parked();
    window
        .read_with(&visual, |view, cx| {
            assert!(view.root.read(cx).pane_hovered.is_none())
        })
        .unwrap();
}
