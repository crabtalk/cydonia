use crate::model::{
    project::Project,
    session::ChatSession,
    settings::{Agent, Settings},
    state::State,
    workspace::Workspace,
};
use bezel::{gpui, theme::Theme};
use gpui::{div, prelude::*};
use markdown::AppExt as _;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

static OPENED: Mutex<Vec<(String, Option<PathBuf>)>> = Mutex::new(Vec::new());

fn record(url: &str, base: Option<&Path>, _: &mut gpui::Window, _: &mut gpui::App) {
    OPENED
        .lock()
        .unwrap()
        .push((url.to_owned(), base.map(Path::to_path_buf)));
}

struct LinkedTranscript {
    workspace: gpui::Entity<Workspace>,
    focus: gpui::FocusHandle,
    _observe: gpui::Subscription,
}

impl gpui::Render for LinkedTranscript {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let prose = self.workspace.update(cx, |workspace, cx| {
            super::prose(
                workspace.session(1).unwrap(),
                0,
                "Read [source](src/main.rs:12).",
                window,
                cx,
            )
        });
        div().size_full().track_focus(&self.focus).child(prose)
    }
}

#[gpui::test]
fn a_clicked_session_link_reaches_the_handler_with_the_session_folder(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        Theme::install(bezel::theme::Appearance::Light, cx);
        cx.set_link_handler(record);
    });
    let window = cx.add_window(|window, cx| {
        let workspace = cx.new(|cx| Workspace::new(Settings::default(), State::default(), cx));
        workspace.update(cx, |workspace, _| {
            let cwd = PathBuf::from("/work/project");
            let mut project = Project::new(cwd.clone());
            let record = serde_json::from_value(serde_json::json!({
                "id": "test", "agent": "test", "title": "", "name": null,
                "updated": 1, "items": []
            }))
            .unwrap();
            project.sessions.push(ChatSession::restore(
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
            ));
            workspace.projects.push(project);
        });
        let observe = cx.observe(&workspace, |_, _, cx| cx.notify());
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        LinkedTranscript {
            workspace,
            focus,
            _observe: observe,
        }
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    let point = window
        .update(&mut visual, |view, _, cx| {
            let layouts = view
                .workspace
                .read(cx)
                .session(1)
                .unwrap()
                .transcript
                .layouts(0);
            let from = markdown::Cursor::new(0, markdown::Part::default(), 5);
            let to = markdown::Cursor { offset: 6, ..from };
            layouts.rects(markdown::Selection::new(from, to))[0].center()
        })
        .unwrap();
    visual.simulate_mouse_move(point, None, gpui::Modifiers::default());
    visual.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    visual.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    assert_eq!(
        *OPENED.lock().unwrap(),
        [(
            "src/main.rs:12".to_owned(),
            Some(PathBuf::from("/work/project"))
        )]
    );
}
