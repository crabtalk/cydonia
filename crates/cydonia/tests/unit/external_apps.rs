use super::*;
use std::os::unix::process::ExitStatusExt;

#[test]
fn application_and_file_paths_are_passed_as_literal_arguments() {
    let app = Path::new("/Applications/Visual Studio Code.app");
    let file = Path::new("/work/a file;$(echo ignored).rs");
    let command = open_command(file, &Target::Application(app.to_path_buf()));
    assert_eq!(command.get_program(), "/usr/bin/open");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "-a".as_ref(),
            app.as_os_str(),
            "--".as_ref(),
            file.as_os_str()
        ]
    );
}

#[test]
fn command_output_and_launch_errors_are_preserved() {
    let success = |stdout: &[u8]| std::process::Output {
        status: std::process::ExitStatus::from_raw(0),
        stdout: stdout.to_vec(),
        stderr: vec![],
    };
    assert_eq!(output(success(b"\n")).unwrap(), "");
    assert_eq!(
        output(success(b"/Applications/Editor.app/\n")).unwrap(),
        "/Applications/Editor.app/"
    );
    let error = output(std::process::Output {
        status: std::process::ExitStatus::from_raw(256),
        stdout: vec![],
        stderr: b"Application could not be opened\n".to_vec(),
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "Application could not be opened");
}

#[test]
fn terminal_opens_the_containing_directory_and_folder_reveals_the_file() {
    let path = Path::new("/work/my project/src/main.rs");
    let args = |target| {
        open_command(path, &target)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        args(Target::Default),
        ["--", "/work/my project/src/main.rs"]
    );
    assert_eq!(
        args(Target::Terminal),
        ["-a", "Terminal", "--", "/work/my project/src"]
    );
    assert_eq!(
        args(Target::Folder),
        ["-R", "--", "/work/my project/src/main.rs"]
    );
}

struct ExternalBar {
    file: gpui::Entity<FileView>,
    _observe: gpui::Subscription,
}
impl Render for ExternalBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bar = self
            .file
            .update(cx, |file, cx| file.status_bar(false, window, cx));
        div().size_full().flex().flex_col().justify_end().child(bar)
    }
}

#[gpui::test]
fn external_icon_toggles_the_app_menu_without_reopening(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| Theme::install(bezel::theme::Appearance::Light, cx));
    let file = cx.new(|cx| {
        let mut file = FileView::new("/tmp/cydonia-menu-example.rs".into(), cx);
        file.external_menu.loaded = true;
        file.external_menu.apps = vec![Application {
            kind: String::new(),
            icon: String::new(),
            image: None,
            name: "Zed".into(),
            path: "/Applications/Zed.app".into(),
        }];
        file
    });
    let window = cx.add_window(|_, cx| {
        let observe = cx.observe(&file, |_, _, cx| cx.notify());
        ExternalBar {
            file: file.clone(),
            _observe: observe,
        }
    });
    let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    for expected in [true, false, true, false] {
        let point = visual.debug_bounds("file-open-with").unwrap().center();
        visual.simulate_click(point, gpui::Modifiers::default());
        visual.run_until_parked();
        assert_eq!(
            file.read_with(&visual, |file, _| file.external_menu.open),
            expected
        );
    }
}
