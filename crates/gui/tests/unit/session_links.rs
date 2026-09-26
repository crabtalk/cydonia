use super::resolve;
use std::path::{Path, PathBuf};

#[test]
fn local_links_resolve_against_the_session_project() {
    let cwd = Path::new("/work/project");
    for (href, expected, line) in [
        (
            "src/routes/next/+page.svelte",
            "/work/project/src/routes/next/+page.svelte",
            None,
        ),
        ("./src/main.rs:42", "/work/project/src/main.rs", Some(42)),
        ("src/main.rs:42:7", "/work/project/src/main.rs", Some(42)),
        ("/other/main.rs#L12", "/other/main.rs", Some(12)),
        ("src/main.rs#L12-L20", "/work/project/src/main.rs", Some(12)),
        ("../shared/a.rs", "/work/shared/a.rs", None),
        ("file:///work/my%20file.rs#L3", "/work/my file.rs", Some(3)),
        ("docs/my%20file.md", "/work/project/docs/my file.md", None),
    ] {
        assert_eq!(
            resolve(cwd, href),
            Some((PathBuf::from(expected), line)),
            "{href}"
        );
    }
}

#[test]
fn external_links_and_anchors_are_not_file_requests() {
    for href in [
        "https://example.com/a:80",
        "http://example.com",
        "mailto:user@example.com",
        "cydonia://session/context",
        "#heading",
        "//example.com/file",
        "",
    ] {
        assert_eq!(resolve(Path::new("/work/project"), href), None, "{href}");
    }
}

use crate::model::{
    project::Project,
    session::ChatSession,
    settings::{Agent, Settings},
    state::State,
    workspace::Workspace,
};
use bezel::{gpui, theme::Theme};
use gpui::{div, prelude::*};

struct LinkedTranscript {
    workspace: gpui::Entity<Workspace>,
    opened: Vec<super::OpenSessionFile>,
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
            super::super::prose(
                workspace.session(1).unwrap(),
                0,
                "Read [source](src/main.rs:12).",
                window,
                cx,
            )
        });
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, action: &super::OpenSessionFile, _, _| {
                this.opened.push(action.clone())
            }))
            .child(prose)
    }
}

#[gpui::test]
fn clicking_a_session_file_link_dispatches_an_internal_open(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
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
            opened: vec![],
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
    window
        .update(&mut visual, |view, _, _| {
            assert_eq!(
                view.opened,
                vec![super::OpenSessionFile {
                    session: 1,
                    path: PathBuf::from("/work/project/src/main.rs"),
                    line: Some(12)
                }]
            );
        })
        .unwrap();
}
