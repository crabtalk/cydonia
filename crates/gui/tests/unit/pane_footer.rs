use super::*;
use crate::model::{
    project::Project,
    session::ChatSession,
    settings::{Agent, Settings},
    state,
};
use bezel::gpui::{self, Entity, Render};

struct PaneView(Entity<Cydonia>, Member);

impl Render for PaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.0.update(cx, |root, cx| {
            root.sync_items(cx);
            root.pane(&self.1, window, cx)
        });
        div().w(px(600.)).h(px(500.)).child(pane)
    }
}

#[gpui::test]
fn arranged_session_measures_its_own_composer(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let window = cx.add_window(|window, cx| {
        let root = cx.new(|cx| Cydonia::new(Settings::default(), state::State::default(), window, cx));
        let member = root.update(cx, |root, cx| {
            let member = root.workspace.update(cx, |workspace, cx| {
                let cwd = std::path::PathBuf::from("/nonexistent/cydonia-pane-footer");
                let mut project = Project::new(cwd.clone(), cx);
                for id in [1, 2] {
                    let record = serde_json::from_value(serde_json::json!({
                        "id":id.to_string(), "agent":"test", "title":"", "name":null, "updated":1, "items":[]
                    })).unwrap();
                    project.sessions.push(crate::model::project::HeldSession::new(ChatSession::restore(id, cwd.clone(), Agent {
                        name: "test".into(), id: None, command: String::new(), args: vec![], env: Default::default(),
                    }, record), cx));
                }
                project.active = Some(1);
                workspace.projects.push(project);
                workspace.active = Some(0);
                workspace.member_of(0, Showing::Session(2), cx).unwrap()
            });
            let tab = root.leaves[0].clone();
            tab.update(cx, |tab, _| tab.entry = Some(member.clone()));
            // The pane a space would hold this tab in.
            let (workspace, owner) = (root.workspace.clone(), cx.entity().downgrade());
            let pane = cx.new(|cx| crate::view::pane::Pane::new(member.clone(), workspace, owner, cx));
            pane.update(cx, |pane, _| pane.tabs = vec![tab.clone()]);
            root.panes.push(pane);
            let composer = tab.read(cx).composer.clone();
            composer.update(cx, |composer, cx| {
                composer.set_session(Some(2), "line\nline\nline\nline\nline\nline", cx);
            });
            member
        });
        PaneView(root, member)
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for _ in 0..3 {
        visual.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        visual.run_until_parked();
    }
    window
        .update(&mut visual, |view, _, cx| {
            let root = view.0.read(cx);
            let height = |id| {
                root.session_views(id, cx)
                    .first()
                    .map_or(px(0.), |view| view.read(cx).state.footer_height.get())
            };
            assert!(height(2) > px(crate::view::window::root::composer_height()));
            assert_eq!(height(1), px(0.));
        })
        .unwrap();
}
