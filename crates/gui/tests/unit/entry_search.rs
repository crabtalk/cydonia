use crate::{
    model::{project::Project, settings::Settings, state::State, workspace::Workspace},
    view::kit::{
        entry_search::{self, Hit},
        link::mention,
    },
};
use artifact::project::{Project as _, fs};
use bezel::gpui::{self, AppContext as _};

#[test]
fn a_short_line_is_kept_whole_with_the_match_where_it_was() {
    let (text, at) = entry_search::clipped("  the pump runs", 6..10);
    assert_eq!(text.as_ref(), "the pump runs");
    assert_eq!(&text[at], "pump");
}

#[test]
fn a_long_line_is_cut_ahead_of_the_match_and_says_so() {
    let line = format!("{}needle and after", "word ".repeat(40));
    let start = line.find("needle").unwrap();
    let (text, at) = entry_search::clipped(&line, start..start + 6);
    assert!(text.starts_with('…'), "{text}");
    assert_eq!(&text[at], "needle");
}

#[gpui::test]
fn local_references_keep_the_canvas_project_and_heading(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let dir = std::env::temp_dir().join(format!("cydonia-entry-picker-{}", std::process::id()));
        let other = dir.join("other");
        let home = dir.join("home");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        fs::Project::new(&other).create_article("# Other").unwrap();
        fs::Project::new(&home)
            .create_article("# Guide\n\n## Setup\n\nInstructions.")
            .unwrap();
        let workspace = cx.new(|cx| Workspace::new(Settings::default(), State::default(), cx));
        workspace.update(cx, |workspace, cx| {
            for path in [&other, &home] {
                let project = Project::new(workspace.intern(path), path.clone(), cx);
                workspace.projects.push(project);
            }
            workspace.active = Some(0);
            let entries = mention::read(workspace, cx);
            cx.set_global(entries);
            let project = &workspace.projects[1];
            let number = project.articles[0].number.unwrap();
            let hits = Hit::named(&format!("#{number}#setup"), workspace, Some(project.id), cx);
            let url = hits[0].link(workspace, cx).unwrap();
            assert_eq!(url, format!("cydonia://home#{number}#setup"));
            assert_eq!(workspace.resolve(&url, cx).unwrap().project, project.id);
            let explicit = Hit::named(&format!("other#{number}"), workspace, Some(project.id), cx);
            assert_eq!(
                explicit[0].link(workspace, cx).unwrap(),
                format!("cydonia://other#{number}")
            );
        });
        std::fs::remove_dir_all(dir).unwrap();
    });
}
